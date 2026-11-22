#include "RustelAudio.h"
#include "RustelLink.h"
#include "IAudioReceiver.h"
#include "ModularSynth.h"
#include "OpenFrameworksPort.h"
#include "FileStream.h"
#include <array>
#include <algorithm>
#include <cstring>
#include <atomic>
#include <cmath>
#include <mutex>
#include <set>
#if JUCE_WINDOWS
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#undef LoadString
#undef min
#undef max
#endif

// The worker opens mappings, adapts sample rates and buffers PCM. Process only
// consumes this fixed SPSC queue; it never opens a handle, takes a lock or waits.
class RustelAudioTransport : private juce::Thread {
   static constexpr uint64_t QueueSize = 16384;
   struct Frame { float left, right; uint64_t epoch; };
   struct Config {
      std::string mapping;
      int slot{-1}, rate{48000}, block{256};
      bool active{false};
      uint64_t epoch{0};
   };
public:
   RustelAudioTransport() : Thread("Rustel lane PCM") { startThread(); }
   ~RustelAudioTransport() override { signalThreadShouldExit(); notify(); stopThread(3000); }
   void Configure(const std::string& mapping, int slot, bool active) {
      std::lock_guard<std::mutex> lock(mMutex);
      int rate = int(gSampleRate), block = gBufferSize;
      if (mConfig.mapping == mapping && mConfig.slot == slot && mConfig.active == active && mConfig.rate == rate && mConfig.block == block) return;
      mConfig = {mapping,slot,rate,block,active,mConfig.epoch+1};
      mEpoch.store(mConfig.epoch,std::memory_order_release);
   }
   bool Pop(float& left, float& right) {
      auto read = mRead.load(std::memory_order_relaxed);
      auto write = mWrite.load(std::memory_order_acquire);
      auto epoch = mEpoch.load(std::memory_order_acquire);
      // Discard old bindings without ever moving the producer's cursor.
      while (read < write) {
         const auto& frame = mQueue[read & (QueueSize-1)];
         ++read;
         if (frame.epoch == epoch) {
            left=frame.left;right=frame.right;
            mRead.store(read,std::memory_order_release);return true;
         }
      }
      mRead.store(read,std::memory_order_release);return false;
   }
   bool Receiving() const { return mReceiving.load(std::memory_order_relaxed); }
   uint64_t Epoch() const { return mEpoch.load(std::memory_order_acquire); }
private:
   void run() override {
#if JUCE_WINDOWS
      HANDLE handle = nullptr; void* base = nullptr;
      std::string mapped;
      double position = 0;
      uint64_t epoch = 0, sourceEpoch = 0, lastHead = 0, lastHeadAt = 0;
      auto close = [&] { if(base) UnmapViewOfFile(base);if(handle) CloseHandle(handle);base=nullptr;handle=nullptr;mapped.clear();mReceiving.store(false); };
      auto load32 = [&](size_t offset) -> uint32_t { return uint32_t(InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(static_cast<char*>(base)+offset),0,0)); };
      auto load64 = [&](size_t offset) -> uint64_t { return uint64_t(InterlockedCompareExchange64(reinterpret_cast<volatile LONG64*>(static_cast<char*>(base)+offset),0,0)); };
      auto sample = [&](int slot,uint64_t frame,int channel,float& out) {
         size_t at=4096+((size_t(slot)*16384+(frame&16383))*2+channel)*8;
         uint64_t bits=load64(at);
         if(uint32_t(bits>>32)!=uint32_t(frame)) return false;
         uint32_t value=uint32_t(bits);std::memcpy(&out,&value,sizeof(out));
         return std::isfinite(out);
      };
      while(!threadShouldExit()) {
         Config config;{std::lock_guard<std::mutex> lock(mMutex);config=mConfig;}
         if (!config.active || config.slot<0 || config.slot>=15 || config.rate<=0) {close();wait(10);continue;}
         if(mapped!=config.mapping) {
            close();
            if(config.mapping.rfind("Local\\RustelAudio-",0)!=0 || config.mapping.size()>128) {wait(100);continue;}
            handle=OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,juce::String(config.mapping).toWideCharPointer());
            if(handle) base=MapViewOfFile(handle,FILE_MAP_ALL_ACCESS,0,0,4096+15*16384*2*8);
            if(!base || load32(0)!=0x524c4131 || load32(4)!=1 || load32(12)!=15 || load32(16)!=16384) {close();wait(100);continue;}
            mapped=config.mapping;epoch=0;lastHeadAt=GetTickCount64();lastHead=load64(24);
         }
         uint32_t rate=load32(8);uint64_t head=load64(24), se=load64(32), now=GetTickCount64();
         if(head!=lastHead) {lastHead=head;lastHeadAt=now;}
         if(rate<8000 || rate>384000 || now-lastHeadAt>500) {mReceiving.store(false);wait(5);continue;}
         double target=rate*0.05;
         if(epoch!=config.epoch || sourceEpoch!=se || position+2>=double(head) || double(head)-position>12000) {
            if(double(head)<target+2) {wait(2);continue;}
            position=double(head)-target;epoch=config.epoch;sourceEpoch=se;
         }
         // Lease only a healthy, connected consumer. A stopped/deleted node's
         // lease expires automatically; another reader's lease is never cleared.
         InterlockedExchange64(reinterpret_cast<volatile LONG64*>(static_cast<char*>(base)+64+config.slot*64),LONG64(now));
         uint64_t read=mRead.load(std::memory_order_acquire), write=mWrite.load(std::memory_order_relaxed);
         size_t goal=size_t(std::clamp(config.block*3,256,8192));
         double correction=std::clamp((double(head)-position-target)/target*0.002,-0.005,0.005);
         double ratio=double(rate)/config.rate*(1+correction);
         while(write-read<goal && write-read<QueueSize-1 && position+2<double(head)) {
            auto frame=uint64_t(position);float l0,r0,l1,r1;
            if(!sample(config.slot,frame,0,l0)||!sample(config.slot,frame,1,r0)||!sample(config.slot,frame+1,0,l1)||!sample(config.slot,frame+1,1,r1)) {position=double(head)-target;break;}
            float f=float(position-double(frame));
            mQueue[write&(QueueSize-1)]={l0+(l1-l0)*f,r0+(r1-r0)*f,epoch};
            ++write;position+=ratio;
         }
         mWrite.store(write,std::memory_order_release);mReceiving.store(true);wait(2);
         // Optional diagnostics for off-thread probes: frames consumed by the
         // real Bespoke audio callback, not just copied into its queue.
         InterlockedExchange64(reinterpret_cast<volatile LONG64*>(static_cast<char*>(base)+72+config.slot*64),LONG64(mRead.load(std::memory_order_acquire)));
      }
      close();
#else
      while(!threadShouldExit()) wait(100);
#endif
   }
   std::mutex mMutex;
   Config mConfig;
   std::array<Frame,QueueSize> mQueue{};
   std::atomic<uint64_t> mRead{0},mWrite{0},mEpoch{0};
   std::atomic<bool> mReceiving{false};
};

RustelAudio::RustelAudio() : mTransport(std::make_unique<RustelAudioTransport>()),mLeft(gBufferSize),mRight(gBufferSize) { GetVizBuffer()->SetNumChannels(2); }
RustelAudio::~RustelAudio() = default;
void RustelAudio::CreateUIControls() {
   IDrawableModule::CreateUIControls();
   mLanes=new DropdownList(this,"lane",4,4,&mSelected,350);
}
void RustelAudio::DropdownUpdated(DropdownList*,int,double) {
   mKey=mSelected>=0 && mSelected<int(mChoices.size())?mChoices[mSelected].key:"";
}
void RustelAudio::Poll() {
   std::vector<Choice> choices;
   auto sessions=RustelLink::Sessions();
   if(auto* list=sessions.getArray()) for(const auto& session:*list) {
      auto stream=session["lane_audio"];auto* routes=stream["lanes"].getDynamicObject();
      if(!routes) continue;
      auto mapping=stream["mapping"].toString().toStdString();
      std::set<std::string> added;
      auto add=[&](const std::string& id,const std::string& text) {
         auto property=juce::Identifier(id);
         if(!routes->hasProperty(property)) return;
         int slot=int(routes->getProperty(property));if(slot<0||slot>=15) return;
         choices.push_back({session["session"].toString().toStdString()+"/"+id,session["session"].toString().toStdString()+" / "+id+" "+text,mapping,slot});added.insert(id);
      };
      if(auto* lanes=session["lanes"].getArray()) for(const auto& lane:*lanes) {
         auto source=lane["source"].toString().upToFirstOccurrenceOf("\n",false,false).substring(0,70).toStdString();
         add(lane["id"].toString().toStdString(),source);
      }
      // Keep retired lanes selectable while their existing effects ring out.
      for(int i=0;i<routes->getProperties().size();++i) {
         auto id=routes->getProperties().getName(i).toString().toStdString();
         if(!added.count(id)) add(id,"(retired lane / tails)");
      }
   }
   bool changed=choices.size()!=mChoices.size();
   if(!changed) for(size_t i=0;i<choices.size();++i) changed|=choices[i].key!=mChoices[i].key || choices[i].label!=mChoices[i].label || choices[i].mapping!=mChoices[i].mapping;
   if(changed) {
      mChoices=choices;mLanes->Clear();mSelected=-1;
      for(int i=0;i<int(choices.size());++i) {mLanes->AddLabel(choices[i].label,i);if(choices[i].key==mKey)mSelected=i;}
   }
   if(mSelected>=0 && mSelected<int(mChoices.size())) {
      auto& choice=mChoices[mSelected];mTransport->Configure(choice.mapping,choice.slot,mEnabled && GetTarget()!=nullptr);
   } else mTransport->Configure("",-1,false);
}
void RustelAudio::Process(double) {
   if(mLeft.size()!=size_t(gBufferSize)) return;
   SyncOutputBuffer(2);
   auto* target=mEnabled?GetTarget():nullptr;
   auto epoch=mTransport->Epoch();
   if(epoch!=mEpoch) { mEpoch=epoch;mFade=0;mLastLeft=mLastRight=0; }
   for(int i=0;i<gBufferSize;++i) {
      float left=0,right=0;
      if(mTransport->Pop(left,right) && target) {
         mFade=std::min(1.0f,mFade+1.0f/64);mLastLeft=left;mLastRight=right;
      } else mFade=std::max(0.0f,mFade-1.0f/64);
      mLeft[i]=mLastLeft*mFade;mRight[i]=mLastRight*mFade;
   }
   if(target) {Add(target->GetBuffer()->GetChannel(0),mLeft.data(),gBufferSize);Add(target->GetBuffer()->GetChannel(1),mRight.data(),gBufferSize);}
   GetVizBuffer()->WriteChunk(mLeft.data(),gBufferSize,0);GetVizBuffer()->WriteChunk(mRight.data(),gBufferSize,1);
}
void RustelAudio::DrawModule() {
   mLanes->Draw();ofSetColor(165,190,210);
   DrawTextNormal(mChoices.empty()?"enable RUSTEL_LANE_AUDIO=1 in Rustel":mTransport->Receiving()?"stereo PCM / local monitor muted / tails active":"select lane and connect an audio cable",5,39,10);
}
void RustelAudio::LoadLayout(const ofxJSONElement& info) {mModuleSaveData.LoadString("target",info);mModuleSaveData.LoadString("binding",info);SetUpFromSaveData();}
void RustelAudio::SetUpFromSaveData() {SetTarget(TheSynth->FindModule(mModuleSaveData.GetString("target")));mKey=mModuleSaveData.GetString("binding");}
void RustelAudio::SaveState(FileStreamOut& out) {out<<GetModuleSaveStateRev();IDrawableModule::SaveState(out);out<<mKey;}
void RustelAudio::LoadState(FileStreamIn& in,int rev) {IDrawableModule::LoadState(in,rev);LoadStateValidate(rev<=GetModuleSaveStateRev());in>>mKey;}
