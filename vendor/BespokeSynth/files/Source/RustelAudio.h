#pragma once
#include "IAudioSource.h"
#include "IDrawableModule.h"
#include "DropdownList.h"
#include <memory>
#include <vector>

class RustelAudioTransport;
class RustelAudio : public IAudioSource, public IDrawableModule, public IDropdownListener
{
public:
   RustelAudio();
   ~RustelAudio() override;
   static IDrawableModule* Create() { return new RustelAudio(); }
   static bool AcceptsAudio() { return false; }
   static bool AcceptsNotes() { return false; }
   static bool AcceptsPulses() { return false; }
   void CreateUIControls() override;
   void Poll() override;
   void Process(double time) override;
   void SetEnabled(bool enabled) override { mEnabled = enabled; }
   bool IsEnabled() const override { return mEnabled; }
   void DropdownUpdated(DropdownList*, int, double) override;
   void LoadLayout(const ofxJSONElement&) override;
   void SetUpFromSaveData() override;
   void SaveState(FileStreamOut&) override;
   void LoadState(FileStreamIn&, int) override;
   int GetModuleSaveStateRev() const override { return 0; }
private:
   struct Choice { std::string key, label, mapping; int slot; };
   void DrawModule() override;
   void GetModuleDimensions(float& w, float& h) override { w = 360; h = 58; }
   std::unique_ptr<RustelAudioTransport> mTransport;
   DropdownList* mLanes{nullptr};
   int mSelected{-1};
   std::string mKey;
   std::vector<Choice> mChoices;
   std::vector<float> mLeft, mRight;
   float mLastLeft{0}, mLastRight{0}, mFade{0};
   uint64_t mEpoch{0};
};
