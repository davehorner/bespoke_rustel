#pragma once
// Standalone IPC reader used by the native node and cross-platform contract tests.
#include <cstdint>
#include <cstring>
#include <string>
#include <climits>
#include <cstdlib>
#if defined(_WIN32)
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#undef LoadString
#undef min
#undef max
#else
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <signal.h>
#include <time.h>
#include <cerrno>
#endif

class RustelAudioShared
{
public:
   static constexpr size_t Bytes = 4096 + 15 * 16384 * 2 * 8;
   ~RustelAudioShared() { Close(); }
   RustelAudioShared() = default;
   RustelAudioShared(const RustelAudioShared&) = delete;
   RustelAudioShared& operator=(const RustelAudioShared&) = delete;
   static uint64_t NowMillis() {
#if defined(_WIN32)
      return GetTickCount64();
#else
      timespec now{};
      if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return 0;
      return uint64_t(now.tv_sec)*1000 + uint64_t(now.tv_nsec)/1000000;
#endif
   }
   bool Open(const std::string& name) {
      Close();
#if defined(_WIN32)
      if(name.rfind("Local\\RustelAudio-",0)!=0 || name.size()>128) return false;
      std::wstring wide(name.begin(),name.end()); // Generated mapping names are ASCII.
      mHandle=OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,wide.c_str());
      if(mHandle) mBase=MapViewOfFile(mHandle,FILE_MAP_ALL_ACCESS,0,0,Bytes);
#else
      if(name.rfind("/rla-",0)!=0 || name.size()>31) return false;
      auto separator=name.find('-',5);
      if(separator==std::string::npos || separator==5 || separator+1==name.size()) return false;
      for(size_t i=5;i<name.size();++i) if(i!=separator && !((name[i]>='0'&&name[i]<='9')||(name[i]>='a'&&name[i]<='f'))) return false;
      char* end=nullptr;
      auto pid=std::strtoul(name.c_str()+5,&end,16);
      if(end!=name.c_str()+separator || pid==0 || pid>INT_MAX) return false;
      int fd=shm_open(name.c_str(),O_RDWR,0);
      if(fd<0) return false;
      fcntl(fd,F_SETFD,FD_CLOEXEC);
      struct stat status{};
      // Darwin rounds shm allocations to a VM page. Map only the protocol's
      // required bytes; reject undersized objects, not harmless trailing padding.
      if(fstat(fd,&status)!=0 || status.st_size<static_cast<off_t>(Bytes) || status.st_uid!=geteuid()) {close(fd);return false;}
      void* base=mmap(nullptr,Bytes,PROT_READ|PROT_WRITE,MAP_SHARED,fd,0);
      close(fd);
      if(base==MAP_FAILED) return false;
      mBase=base;mProducer=static_cast<pid_t>(pid);
#endif
      if(!mBase) {Close();return false;}
      mName=name;
      if(Load32(0)!=0x524c4131 || Load32(4)!=1 || Load32(12)!=15 || Load32(16)!=16384) {Close();return false;}
      return true;
   }
   bool ProducerAlive() const {
#if defined(_WIN32)
      return true; // Windows reclaims unnamed lifetime automatically on final close.
#else
      return mProducer>0 && (kill(mProducer,0)==0 || errno==EPERM);
#endif
   }
   void Close() {
#if defined(_WIN32)
      if(mBase) UnmapViewOfFile(mBase);
      if(mHandle) CloseHandle(mHandle);
      mHandle=nullptr;
#else
      // A dead producer cannot unlink its object. Existing mappings remain valid
      // until their readers close; never unlink a live producer's stream.
      if(mBase && !mName.empty() && !ProducerAlive()) shm_unlink(mName.c_str());
      if(mBase) munmap(mBase,Bytes);
      mProducer=0;
#endif
      mBase=nullptr;mName.clear();
   }
   uint32_t Load32(size_t offset) const {
#if defined(_WIN32)
      return uint32_t(InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(static_cast<char*>(mBase)+offset),0,0));
#else
      static_assert(__atomic_always_lock_free(8,nullptr),"Lane PCM requires lock-free 64-bit atomics");
      return __atomic_load_n(reinterpret_cast<const uint32_t*>(static_cast<char*>(mBase)+offset),__ATOMIC_ACQUIRE);
#endif
   }
   uint64_t Load64(size_t offset) const {
#if defined(_WIN32)
      return uint64_t(InterlockedCompareExchange64(reinterpret_cast<volatile LONG64*>(static_cast<char*>(mBase)+offset),0,0));
#else
      return __atomic_load_n(reinterpret_cast<const uint64_t*>(static_cast<char*>(mBase)+offset),__ATOMIC_ACQUIRE);
#endif
   }
   void Store64(size_t offset,uint64_t value) {
#if defined(_WIN32)
      InterlockedExchange64(reinterpret_cast<volatile LONG64*>(static_cast<char*>(mBase)+offset),LONG64(value));
#else
      __atomic_store_n(reinterpret_cast<uint64_t*>(static_cast<char*>(mBase)+offset),value,__ATOMIC_RELEASE);
#endif
   }
private:
   void* mBase{nullptr};
   std::string mName;
#if defined(_WIN32)
   HANDLE mHandle{nullptr};
#else
   pid_t mProducer{0};
#endif
};
