#pragma once
#include <juce_core/juce_core.h>

#include "IDrawableModule.h"
#include "DropdownList.h"
#include "Slider.h"
#include "PatchCableSource.h"

// Special cables describe remote pattern operations; they carry no audio.
class RustelTranspose : public IDrawableModule, public IIntSliderListener
{
public:
   static IDrawableModule* Create() { return new RustelTranspose(); }
   void CreateUIControls() override;
   void IntSliderUpdated(IntSlider*, int, double) override {}
   int Semitones() { ComputeSliders(0); return mSemitones; }
private:
   void DrawModule() override;
   void GetModuleDimensions(float& width, float& height) override { width = 170; height = 40; }
   int mSemitones{0};
   IntSlider* mSlider{nullptr};
};

class RustelPattern : public IDrawableModule, public IDropdownListener
{
public:
   static IDrawableModule* Create() { return new RustelPattern(); }
   ~RustelPattern() override;
   void CreateUIControls() override;
   void Poll() override;
   void DropdownUpdated(DropdownList*, int, double) override;
   void SaveState(FileStreamOut& out) override;
   void LoadState(FileStreamIn& in, int rev) override;
   int GetModuleSaveStateRev() const override { return 0; }
private:
   void DrawModule() override;
   void GetModuleDimensions(float& width, float& height) override { width = 300; height = 62; }
   DropdownList* mLanes{nullptr};
   PatchCableSource* mPatternOutput{nullptr};
   int mSelected{-1};
   std::vector<std::string> mKeys;
   std::vector<std::string> mLabels;
   juce::var mLastBinding;
   double mRetryAt{0};
   std::string mSelectedKey;
   std::string mLastCommand;
   std::string mStatus{"waiting for music hub"};
};
