#include "RustelPattern.h"
#include "OpenFrameworksPort.h"
#include "FileStream.h"
#include "RustelLink.h"
#include "ModularSynth.h"
#include <deque>
#include <mutex>

namespace
{
   // One shared network thread for every pattern node. HTTP adapts the JUCE
   // client to Hearsay in music-hub; neither Poll nor audio callbacks do I/O.
   class MusicConnection : private juce::Thread
   {
   public:
      MusicConnection()
      : Thread("Rustel music link")
      {
         startThread();
      }
      ~MusicConnection() override
      {
         signalThreadShouldExit();
         notify();
         stopThread(5000);
      }
      static MusicConnection& Get()
      {
         static MusicConnection instance;
         return instance;
      }
      juce::var Sessions()
      {
         std::lock_guard<std::mutex> lock(mMutex);
         return mSessions;
      }
      juce::var Analysis()
      {
         std::lock_guard<std::mutex> lock(mMutex);
         return mAnalysis;
      }
      std::string Status()
      {
         std::lock_guard<std::mutex> lock(mMutex);
         return mStatus;
      }
      void Send(juce::var command)
      {
         std::lock_guard<std::mutex> lock(mMutex);
         if (mCommands.size() < 64)
            mCommands.push_back(command);
         notify();
      }

   private:
      juce::var Request(const char* path, const juce::var& body = {})
      {
         juce::URL url(juce::String("http://127.0.0.1:9614") + path);
         const bool post = !body.isVoid();
         if (post)
            url = url.withPOSTData(juce::JSON::toString(body));
         int status = 0;
         auto options = juce::URL::InputStreamOptions(post ? juce::URL::ParameterHandling::inPostData : juce::URL::ParameterHandling::inAddress)
                        .withConnectionTimeoutMs(1600)
                        .withStatusCode(&status)
                        .withNumRedirectsToFollow(0)
                        .withExtraHeaders(post ? "Content-Type: application/json\r\n" : "");
         auto stream = url.createInputStream(options);
         if (stream == nullptr)
            return {};
         auto result = juce::JSON::parse(stream->readEntireStreamAsString());
         if (status != 200)
            return {};
         return result;
      }
      void run() override
      {
         double nextSessions = 0;
         while (!threadShouldExit())
         {
            double now = juce::Time::getMillisecondCounterHiRes();
            if (now >= nextSessions)
            {
               auto sessions = Request("/sessions");
               nextSessions = now + 200;
               std::lock_guard<std::mutex> lock(mMutex);
               mSessions = sessions;
               mStatus = sessions.isArray() ? "connected" : "music hub unavailable";
            }
            auto analysis = Request("/analysis");
            std::deque<juce::var> commands;
            {
               std::lock_guard<std::mutex> lock(mMutex);
               mAnalysis = analysis;
               commands.swap(mCommands);
            }
            for (const auto& command : commands)
            {
               if (threadShouldExit())
                  break;
               auto response = Request(command.hasProperty("solo") ? "/solo" : "/transpose", command);
               std::lock_guard<std::mutex> lock(mMutex);
               mStatus = response.isVoid() ? "command timed out; reconnect to retry" : (bool(response["accepted"]) ? "command applied" : response["message"].toString().toStdString());
            }
            wait(33);
         }
      }
      std::mutex mMutex;
      juce::var mSessions;
      juce::var mAnalysis;
      std::deque<juce::var> mCommands;
      std::string mStatus;
   };
}

juce::var RustelLink::Sessions() { return MusicConnection::Get().Sessions(); }
juce::var RustelLink::Analysis() { return MusicConnection::Get().Analysis(); }
std::string RustelLink::Status() { return MusicConnection::Get().Status(); }
void RustelLink::Solo(const std::string& session, int64_t generation, const std::string& lane, bool enabled)
{
   if (session.empty() || lane.empty() || generation < 0) return;
   auto* object = new juce::DynamicObject();
   object->setProperty("request", juce::Uuid().toString());
   object->setProperty("session", juce::String(session));
   object->setProperty("generation", juce::int64(generation));
   object->setProperty("lane", juce::String(lane));
   object->setProperty("solo", enabled);
   MusicConnection::Get().Send(juce::var(object));
}

void RustelTranspose::CreateUIControls()
{
   IDrawableModule::CreateUIControls();
   mSlider = new IntSlider(this, "semitones", 5, 5, 160, 15, &mSemitones, -48, 48);
}
void RustelTranspose::DrawModule()
{
   if (!Minimized() && IsVisible())
      mSlider->Draw();
}

void RustelPattern::CreateUIControls()
{
   IDrawableModule::CreateUIControls();
   mLanes = new DropdownList(this, "lane", 5, 5, &mSelected, 285);
   mPatternOutput = new PatchCableSource(this, kConnectionType_Special);
   mPatternOutput->SetPredicateFilter([](IClickable* target)
                                      {
                                         return dynamic_cast<RustelTranspose*>(target) != nullptr;
                                      });
   AddPatchCableSource(mPatternOutput);
}

void RustelPattern::DropdownUpdated(DropdownList*, int, double)
{
   if (auto* object = mLastBinding.getDynamicObject())
   {
      object->setProperty("semitones", 0);
      object->setProperty("request", juce::Uuid().toString());
      MusicConnection::Get().Send(mLastBinding.clone());
      mLastBinding = juce::var();
   }
   mSelectedKey = mSelected >= 0 && mSelected < int(mKeys.size()) ? mKeys[mSelected] : "";
   mLastCommand.clear();
}

void RustelPattern::LoadLayout(const ofxJSONElement& moduleInfo)
{
   mModuleSaveData.LoadString("target", moduleInfo);
   SetUpFromSaveData();
}

void RustelPattern::SetUpFromSaveData()
{
   // Demo layouts can connect the special pattern cable to a transpose node.
   mPatternOutput->SetTarget(TheSynth->FindModule(mModuleSaveData.GetString("target")));
}

RustelPattern::~RustelPattern()
{
   if (auto* object = mLastBinding.getDynamicObject())
   {
      object->setProperty("semitones", 0);
      object->setProperty("request", juce::Uuid().toString());
      MusicConnection::Get().Send(mLastBinding.clone());
   }
}

void RustelPattern::Poll()
{
   auto& connection = MusicConnection::Get();
   auto sessions = connection.Sessions();
   mStatus = connection.Status();
   std::vector<std::string> keys;
   std::vector<juce::var> bindings;
   std::vector<std::string> labels;
   std::vector<int> offsets;
   if (auto* list = sessions.getArray())
   {
      for (const auto& session : *list)
      {
         if (auto* lanes = session["lanes"].getArray())
         {
            for (const auto& lane : *lanes)
            {
               keys.push_back(session["session"].toString().toStdString() + "/" + lane["id"].toString().toStdString());
               auto label = lane["source"].toString().toStdString();
               if (label.empty())
                  label = lane["label"].toString().toStdString();
               std::replace(label.begin(), label.end(), '\n', ' ');
               if (label.size() > 100)
                  label = label.substr(0, 97) + "...";
               labels.push_back(lane["id"].toString().toStdString() + " " + label);
               offsets.push_back(int(lane["transpose"]));
               juce::DynamicObject::Ptr object = new juce::DynamicObject();
               object->setProperty("session", session["session"]);
               object->setProperty("generation", session["generation"]);
               object->setProperty("lane", lane["id"]);
               bindings.emplace_back(object.get());
            }
         }
      }
   }
   if (keys != mKeys || labels != mLabels)
   {
      mKeys = keys;
      mLabels = labels;
      mLanes->Clear();
      mSelected = -1;
      for (int i = 0; i < int(keys.size()); ++i)
      {
         mLanes->AddLabel(labels[i], i);
         if (keys[i] == mSelectedKey)
            mSelected = i;
      }
   }
   if (mSelected < 0 || mSelected >= int(bindings.size()))
      return;
   auto* transpose = dynamic_cast<RustelTranspose*>(mPatternOutput->GetTarget());
   int semitones = transpose ? transpose->Semitones() : 0;
   auto command = bindings[mSelected].clone();
   command.getDynamicObject()->setProperty("semitones", semitones);
   std::string identity = juce::JSON::toString(command).toStdString();
   double now = juce::Time::getMillisecondCounterHiRes();
   if (identity == mLastCommand && (offsets[mSelected] == semitones || now < mRetryAt))
      return;
   mRetryAt = now + 2000;
   mLastCommand = identity;
   command.getDynamicObject()->setProperty("request", juce::Uuid().toString());
   mLastBinding = command.clone();
   connection.Send(command);
}

void RustelPattern::DrawModule()
{
   if (Minimized() || !IsVisible())
      return;
   mLanes->Draw();
   DrawTextNormal(mStatus, 5, 43);
}

void RustelPattern::SaveState(FileStreamOut& out)
{
   out << GetModuleSaveStateRev();
   IDrawableModule::SaveState(out);
   out << mSelectedKey;
}

void RustelPattern::LoadState(FileStreamIn& in, int rev)
{
   IDrawableModule::LoadState(in, rev);
   LoadStateValidate(rev <= GetModuleSaveStateRev());
   in >> mSelectedKey;
   mKeys.clear();
   mLastCommand.clear();
}
