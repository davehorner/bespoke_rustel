#pragma once

#include "IDrawableModule.h"
#include "DropdownList.h"
#include "Slider.h"
#include "Checkbox.h"
#include "ClickButton.h"
#include <string>
#include <vector>
#include <array>

class RustelView : public IDrawableModule, public IDropdownListener, public IFloatSliderListener, public IButtonListener
{
public:
   RustelView();
   static IDrawableModule* Create() { return new RustelView(); }
   void CreateUIControls() override;
   void Poll() override;
   bool IsResizable() const override { return true; }
   void Resize(float w, float h) override;
   bool CheckNeedsDraw() override { return true; }
   void DropdownUpdated(DropdownList* list, int oldVal, double time) override;
   void FloatSliderUpdated(FloatSlider*, float, double) override {}
   void ButtonClicked(ClickButton*, double) override;
   void CheckboxUpdated(Checkbox*, double) override;
   void MouseReleased() override;
   void SaveState(FileStreamOut& out) override;
   void LoadState(FileStreamIn& in, int rev) override;
   int GetModuleSaveStateRev() const override { return 1; }

private:
   struct Lane
   {
      std::string id, label, source;
      int transpose{ 0 };
   };
   struct Event
   {
      int lane{ 0 };
      double cycle{ 0 }, duration{ 0 };
      float original{ 48 }, pitch{ 48 }, gain{ 0.8f };
      std::vector<std::pair<int, int>> context;
      std::array<double, 4> controls{};
      std::array<bool, 4> hasControl{};
   };
   void DrawModule() override;
   void DrawEventView(const std::vector<int>& visible, double cycle, bool live, float top, float bottom);
   void DrawControls(const std::vector<int>& visible, double cycle, bool live, float top, float bottom);
   float DrawSourceDocument(const std::vector<int>& visible, double cycle, bool live, float y);
   std::string mDocument;
   void PollAudio();
   void DrawAudioView(float top, float bottom, double dt);
   std::vector<float> mAudioScope, mAudioSpectrum;
   std::vector<ofVec2f> mAudioSides;
   std::array<float, 64> mSpectrumBars{}, mSpectrumPeaks{};
   uint64_t mAudioSequence{ 0 }, mAudioStream{ 0 }, mAudioEpoch{ 0 };
   int64_t mAudioGeneration{ -1 };
   int mAudioSampleRate{ 0 };
   double mAudioReceived{ 0 };
   float mDisplayGain{ 4 };
   FloatSlider* mDisplayGainSlider{ nullptr };
   void OnClicked(float x, float y, bool right) override;
   bool MouseMoved(float x, float y) override;
   bool InNoteField(float x, float y);
   bool MouseScrolled(float x, float y, float scrollX, float scrollY, bool isSmoothScroll, bool isInvertedScroll) override;
   float mSourceScroll{ 0 }, mSourceContentHeight{ 0 };
   DropdownList* mSessions{ nullptr };
   DropdownList* mLanePicker{ nullptr };
   DropdownList* mViewPicker{ nullptr };
   int mViewMode{ 0 };
   FloatSlider* mYawSlider{ nullptr };
   FloatSlider* mTiltSlider{ nullptr };
   FloatSlider* mZoomSlider{ nullptr };
   Checkbox* mOrbitCheckbox{ nullptr };
   Checkbox* mSoloCheckbox{ nullptr };
   ClickButton* mResetCamera{ nullptr };
   bool mCameraDragging{ false }, mPanDragging{ false };
   int mDragButton{ 1 };
   float mDragX{ 0 }, mDragY{ 0 }, mPanX{ 0 }, mPanY{ 0 };
   std::vector<std::string> mSessionIds;
   std::vector<Lane> mLanes;
   std::vector<Event> mEvents;
   std::string mSessionId, mLaneId;
   int mSessionIndex{ -1 }, mLaneIndex{ -1 };
   float mYaw{ 0.35f }, mTilt{ 0.6f }, mZoom{ 1 };
   bool mOrbit{ false }, mTraces{ false }, mSolo{ false }, mLive{ false };
   double mCycle{ 0 }, mCps{ 0.5 }, mReceived{ 0 }, mLastFrame{ 0 }, mOrbitPhase{ 0 };
   int64_t mGeneration{ -1 };
   std::string mStatus{ "waiting for music hub" };
};
