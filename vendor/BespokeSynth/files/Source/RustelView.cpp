#include "RustelView.h"
#include "RustelLink.h"
#include "OpenFrameworksPort.h"
#include "FileStream.h"
#include "ModularSynth.h"
#include "RustelNoteGlyph.h"
#include "RustelSourceNavigation.h"
#include "nanovg/nanovg.h"
#include <algorithm>
#include <array>
#include <cmath>

namespace
{
   const std::array<ofColor, 8> kColors{ { { 31, 242, 209 }, { 255, 89, 107 }, { 163, 107, 255 }, { 255, 186, 51 }, { 51, 166, 255 }, { 255, 89, 204 }, { 158, 255, 71 }, { 255, 138, 56 } } };
   struct Point
   {
      float x, y, z;
   };
   struct Screen
   {
      float x, y, depth;
   };
   struct Face
   {
      std::array<Screen, 4> points;
      ofColor color;
      float depth;
      const std::string* note{ nullptr };
      Point center{}, right{}, down{};
      float textScale{ 0 }, textWidth{ 0 }, textHeight{ 0 };
   };
   ofColor WithAlpha(ofColor color, int alpha)
   {
      color.a = alpha;
      return color;
   }

   void Polygon(const std::array<Screen, 4>& points, ofColor color)
   {
      ofFill();
      ofSetColor(color);
      ofBeginShape();
      for (const auto& point : points)
         ofVertex(point.x, point.y);
      ofEndShape(true);
   }
}

RustelView::RustelView()
: IDrawableModule(900, 640)
{}

void RustelView::CreateUIControls()
{
   IDrawableModule::CreateUIControls();
   mSessions = new DropdownList(this, "session", 10, 6, &mSessionIndex, 240);
   mLanePicker = new DropdownList(this, "lane", 260, 6, &mLaneIndex, 180);
   mLanePicker->AddLabel("all lanes", -1);
   mViewPicker = new DropdownList(this, "view", 450, 6, &mViewMode, 180);
   mViewPicker->AddLabel("stage", 0);
   mViewPicker->AddLabel("piano roll", 1);
   mViewPicker->AddLabel("wordfall", 2);
   mViewPicker->AddLabel("spiral", 3);
   mViewPicker->AddLabel("pitch wheel", 4);
   mViewPicker->AddLabel("scope (master)", 5);
   mViewPicker->AddLabel("spectrum (master)", 6);
   mViewPicker->AddLabel("vectorscope (master)", 7);
   mViewPicker->AddLabel("control curves", 8);
   mDisplayGainSlider = new FloatSlider(this, "display gain", 10, 30, 250, 15, &mDisplayGain, 0.25f, 32);
   mYawSlider = new FloatSlider(this, "yaw", 10, 30, 120, 15, &mYaw, -PI, PI);
   mTiltSlider = new FloatSlider(this, "tilt", 140, 30, 120, 15, &mTilt, -1.45f, 1.45f);
   mZoomSlider = new FloatSlider(this, "zoom", 270, 30, 120, 15, &mZoom, 0.2f, 5.0f);
   mOrbitCheckbox = new Checkbox(this, "orbit", 410, 30, &mOrbit);
   mSoloCheckbox = new Checkbox(this, "solo", 480, 30, &mSolo);
   mPartsCheckbox = new Checkbox(this, "parts", 535, 30, &mShowParts);
   mResetCamera = new ClickButton(this, "reset", 595, 30);

}

void RustelView::Resize(float w, float h)
{
   mWidth = std::clamp(w, 640.0f, 1800.0f);
   mHeight = std::clamp(h, 480.0f, 1200.0f);
}

void RustelView::DropdownUpdated(DropdownList* list, int, double)
{
   if (list == mViewPicker)
   {
      mPanX = mPanY = 0;
      mCameraDragging = false;
   }
   if (list == mSessions)
   {
      if (!mSoloLane.empty()) RustelLink::Solo(mSessionId, mGeneration, mSoloLane, false);
      mSolo = false;
      mSoloLane.clear();
      mSessionId = mSessionIndex >= 0 && mSessionIndex < int(mSessionIds.size()) ? mSessionIds[mSessionIndex] : "";
      mGeneration = -1;
      mLaneId.clear();
      mLaneIndex = -1;
      mLanes.clear();
      mExpanded.clear();
      mEvents.clear();
      mLive = false;
      mSourceScroll = 0;
      mSourceFocusFrom = mSourceFocusTo = -1;
      mSourceFocusName.clear(); mSelectedLane.clear(); mSelectedPart.clear();
      mAudioSequence = 0;
      mAudioGeneration = -1;
      mAudioScope.clear();
      mAudioSpectrum.clear();
      mAudioSides.clear();
   }
   if (list == mLanePicker)
   {
      mLaneId = mLaneIndex >= 0 && mLaneIndex < int(mLanes.size()) ? mLanes[mLaneIndex].id : "";
      mSourceScroll = 0;
      if (!mLaneId.empty()) RevealSource(mLaneId, "");
   }
}

void RustelView::Poll()
{
   // Poll copies parsed values; DrawModule only uses native cached data.
   auto sessions = RustelLink::Sessions();
   std::vector<std::string> ids;
   if (auto* list = sessions.getArray())
      for (const auto& session : *list)
         ids.push_back(session["session"].toString().toStdString());
   if (ids != mSessionIds)
   {
      mSessionIds = ids;
      mSessions->Clear();
      mSessionIndex = -1;
      for (int i = 0; i < int(ids.size()); ++i)
      {
         mSessions->AddLabel(ids[i], i);
         if (ids[i] == mSessionId)
            mSessionIndex = i;
      }
      if (mSessionId.empty() && !ids.empty())
      {
         mSessionId = ids[0];
         mSessionIndex = 0;
      }
   }
   mLive = false;
   mStatus = RustelLink::Status();
   if (mSessionIndex < 0 || mSessionIndex >= int(ids.size()))
      return;
   auto session = sessions[mSessionIndex];
   double cycle = double(session["cycle"]), cps = double(session["cps"]);
   auto generation = int64_t(session["generation"]);
   if (!std::isfinite(cycle) || !std::isfinite(cps) || cps <= 0)
      return;
   mLive = true;
   mSoloLane = session["solo_lane"].toString().toStdString();
   mSolo = !mSoloLane.empty();
   PollAudio();
   if (generation == mGeneration && cycle == mCycle)
      return;
   mGeneration = generation;
   const auto document = session["source"].toString().toStdString();
   if (document != mDocument) { mSourceFocusFrom = mSourceFocusTo = -1; mSourceFocusName.clear(); mSelectedLane.clear(); mSelectedPart.clear(); }
   mDocument = document;
   mBindings.clear();
   if (auto* bindings = session["bindings"].getArray())
      for (const auto& binding : *bindings)
         if (auto* rgb = binding["color"].getArray(); rgb && rgb->size() == 3)
            mBindings.push_back({ int(binding["from"]), int(binding["to"]), ofColor(int((*rgb)[0]), int((*rgb)[1]), int((*rgb)[2])), binding["name"].toString().toStdString(), int(binding["definition_from"]), int(binding["definition_to"]) });
   mCycle = cycle;
   mCps = cps;
   mReceived = juce::Time::getMillisecondCounterHiRes();
   std::vector<Lane> lanes;
   if (auto* list = session["lanes"].getArray())
      for (const auto& lane : *list)
      {
         if (lanes.size() >= 64)
            break;
         lanes.push_back({ lane["id"].toString().toStdString(), lane["label"].toString().toStdString(), lane["source"].toString().toStdString(), int(lane["transpose"]) });
         lanes.back().line = std::max(1, int(lane["line"]));
         if (auto* parts = lane["parts"].getArray())
            for (const auto& part : *parts)
               if (auto* rgb = part["color"].getArray(); rgb && rgb->size() == 3 && lanes.back().parts.size() < 64)
                  lanes.back().parts.push_back({ part["name"].toString().toStdString(), ofColor(int((*rgb)[0]), int((*rgb)[1]), int((*rgb)[2])) });
      }
   bool changed = lanes.size() != mLanes.size();
   if (!changed)
      for (size_t i = 0; i < lanes.size(); ++i)
         changed |= lanes[i].id != mLanes[i].id || lanes[i].label != mLanes[i].label || lanes[i].source != mLanes[i].source;
   mLanes = lanes;
   if (changed)
   {
      mLanePicker->Clear();
      mLanePicker->AddLabel("all lanes", -1);
      mLaneIndex = -1;
      for (int i = 0; i < int(lanes.size()); ++i)
      {
         auto label = lanes[i].source.empty() ? lanes[i].label : lanes[i].source;
         std::replace(label.begin(), label.end(), '\n', ' ');
         if (label.size() > 100)
            label = label.substr(0, 97) + "...";
         mLanePicker->AddLabel(lanes[i].id + " " + label, i);
         if (lanes[i].id == mLaneId)
            mLaneIndex = i;
      }
   }
   mEvents.clear();
   if (auto* list = session["events"].getArray())
      for (const auto& event : *list)
      {
         if (mEvents.size() >= 2048)
            break;
         auto id = event["lane"].toString().toStdString();
         auto lane = std::find_if(lanes.begin(), lanes.end(), [&](const Lane& l)
                                  {
                                     return l.id == id;
                                  });
         if (lane == lanes.end())
            continue;
         double begin = double(event["cycle"]), duration = double(event["duration"]);
         float original = event["original_pitch"].isDouble() || event["original_pitch"].isInt() ? float(event["original_pitch"]) : 48;
         float pitch = event["pitch"].isDouble() || event["pitch"].isInt() ? float(event["pitch"]) : 48;
         float gain = float(event["gain"]);
         if (!std::isfinite(begin) || !std::isfinite(duration) || !std::isfinite(original) || !std::isfinite(pitch) || !std::isfinite(gain))
            continue;
         Event parsed{ int(lane - lanes.begin()), begin, duration, original, pitch, std::clamp(gain, 0.0f, 1.0f) };
         const auto pitchValue = event["pitch"];
         parsed.note = RustelNoteGlyph::Format(pitchValue.isDouble() || pitchValue.isInt() || pitchValue.isInt64(), pitch);
         parsed.part = event["part"].toString().substring(0, 64).toStdString();
         if (auto* rgb = event["part_color"].getArray(); rgb && rgb->size() == 3)
            for (int c = 0; c < 3; ++c) parsed.partColor[c] = std::clamp(int((*rgb)[c]), 0, 255);
         if (auto* ranges = event["context"].getArray())
            for (const auto& range : *ranges)
               if (range.size() == 2 && parsed.context.size() < 64)
                  parsed.context.push_back({ int(range[0]), int(range[1]) });
         const char* keys[] = { "gain", "pan", "cutoff", "resonance" };
         auto controls = event["controls"];
         for (int i = 0; i < 4; ++i)
         {
            auto value = controls[keys[i]];
            parsed.hasControl[i] = (value.isDouble() || value.isInt() || value.isInt64()) && std::isfinite(double(value));
            parsed.controls[i] = parsed.hasControl[i] ? double(value) : 0;
         }
         mEvents.push_back(std::move(parsed));
      }
}

std::string RustelView::SoloTarget() const
{
   for (const auto& id : { mSelectedLane, mLaneId })
      if (!id.empty() && std::any_of(mLanes.begin(), mLanes.end(), [&](const Lane& lane) { return lane.id == id; })) return id;
   return "";
}

void RustelView::RevealSource(const std::string& laneId, const std::string& part)
{
   const auto lane = std::find_if(mLanes.begin(), mLanes.end(), [&](const Lane& l) { return l.id == laneId; });
   if (lane == mLanes.end() || mDocument.empty()) return;
   mSelectedLane = laneId;
   mSelectedPart = part;
   if (mSolo && mLive && mSoloLane != laneId) RustelLink::Solo(mSessionId, mGeneration, laneId, true);
   auto expression = juce::String(lane->source).trim();
   auto prefix = expression.upToFirstOccurrenceOf(":", false, false);
   if (expression.contains(":") && prefix.containsOnly("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$"))
      expression = expression.fromFirstOccurrenceOf(":", false, false).trim();
   const Binding* target = nullptr;
   for (const auto& binding : mBindings)
   {
      if (binding.name.empty() || binding.from < 0 || binding.to <= binding.from || size_t(binding.to) > mDocument.size()) continue;
      bool matches = binding.name == part;
      if (part.empty())
      {
         const auto name = juce::String(binding.name);
         if (expression.startsWith(name))
         {
            const auto next = expression[name.length()];
            matches = !juce::CharacterFunctions::isLetterOrDigit(next) && next != '_' && next != '$';
         }
      }
      if (matches) { target = &binding; break; }
   }
   if (target)
   {
      if (target->definitionFrom >= 0 && target->definitionTo > target->definitionFrom && size_t(target->definitionTo) <= mDocument.size())
      {
         mSourceFocusFrom = target->definitionFrom;
         mSourceFocusTo = target->definitionTo;
      }
      else
      {
         const size_t start = RustelSourceNavigation::LineStart(mDocument, size_t(target->from));
         size_t end = mDocument.size();
         for (const auto& binding : mBindings)
         {
            const auto next = RustelSourceNavigation::LineStart(mDocument, size_t(std::max(0, binding.from)));
            if (next > start) end = std::min(end, next);
         }
         for (const auto& candidate : mLanes)
         {
            size_t next = 0;
            for (int line = 1; line < candidate.line && next < mDocument.size(); ++line)
            {
               const auto newline = mDocument.find('\n', next);
               next = newline == std::string::npos ? mDocument.size() : newline + 1;
            }
            if (next > start) end = std::min(end, next);
         }
         mSourceFocusFrom = int(start);
         mSourceFocusTo = int(RustelSourceNavigation::TrimTrailingTrivia(mDocument, start, end));
      }
      mSourceFocusName = target->name;
   }
   else
   {
      size_t offset = 0;
      for (int line = 1; line < lane->line && offset < mDocument.size(); ++line)
      {
         const auto newline = mDocument.find('\n', offset);
         offset = newline == std::string::npos ? mDocument.size() : newline + 1;
      }
      size_t end = mDocument.size();
      for (const auto& binding : mBindings)
      {
         const auto next = RustelSourceNavigation::LineStart(mDocument, size_t(std::max(0, binding.from)));
         if (next > offset) end = std::min(end, next);
      }
      for (const auto& candidate : mLanes)
      {
         if (candidate.line <= lane->line) continue;
         size_t next = 0;
         for (int line = 1; line < candidate.line && next < mDocument.size(); ++line)
         {
            const auto newline = mDocument.find('\n', next);
            next = newline == std::string::npos ? mDocument.size() : newline + 1;
         }
         end = std::min(end, next);
      }
      mSourceFocusFrom = int(offset);
      mSourceFocusTo = int(RustelSourceNavigation::TrimTrailingTrivia(mDocument, offset, end));
      mSourceFocusName = laneId;
   }
   const float charWidth = std::max(1.0f, gFontFixedWidth.GetStringWidth("M", 11));
   const int columns = std::max(10, int((mWidth - 42) / charWidth));
   const int row = RustelSourceNavigation::VisualRow(mDocument, size_t(mSourceFocusFrom), columns);
   const float viewHeight = std::clamp(mHeight * 0.35f, 140.0f, 300.0f) - 8;
   mSourceScroll = std::max(0.0f, 36 + row * 16 - viewHeight * 0.4f);
}

int RustelView::SelectedRow() const
{
   int parent = -1;
   for (int row = 0; row < int(mRows.size()); ++row)
      if (mLanes[mRows[row].lane].id == mSelectedLane)
      {
         if (!mRows[row].child) parent = row;
         else if (!mSelectedPart.empty() && mRows[row].part == mSelectedPart) return row;
      }
   return parent;
}

int RustelView::EventRow(const Event& event) const
{
   int parent = -1;
   for (int i = 0; i < int(mRows.size()); ++i)
      if (mRows[i].lane == event.lane)
      {
         if (!mRows[i].child) parent = i;
         else if (mRows[i].part == event.part) return i;
      }
   return parent;
}

ofColor RustelView::RowColor(int row) const
{
   const auto& track = mRows[row];
   if (track.child)
      for (const auto& part : mLanes[track.lane].parts)
         if (part.name == track.part) return part.color;
   return kColors[track.lane % kColors.size()];
}

void RustelView::DrawModule()
{
   if (Minimized() || !IsVisible())
      return;
   ComputeSliders(0);
   ofPushStyle();
   ofFill();
   ofSetColor(5, 9, 18);
   ofRect(0, 0, mWidth, mHeight, 0);
   mSessions->Draw();
   mLanePicker->Draw();
   mViewPicker->Draw();
   mYawSlider->SetShowing(mViewMode == 0);
   mTiltSlider->SetShowing(mViewMode == 0);
   mOrbitCheckbox->SetShowing(mViewMode == 0);

   mDisplayGainSlider->SetShowing((mViewMode >= 5 && mViewMode <= 7));
   mDisplayGainSlider->Draw();
   mYawSlider->Draw();
   mTiltSlider->Draw();
   mZoomSlider->Draw();
   mOrbitCheckbox->Draw();
   mSoloCheckbox->Draw();
   mPartsCheckbox->Draw();
   mResetCamera->Draw();
   const double now = juce::Time::getMillisecondCounterHiRes();
   const double dt = mLastFrame > 0 ? std::clamp((now - mLastFrame) / 1000.0, 0.0, 0.1) : 0;
   mLastFrame = now;
   if (mOrbit)
      mOrbitPhase += dt * 0.15;
   const double age = std::max(0.0, (now - mReceived) / 1000.0);
   const bool live = mLive && age < 3.0;
   const double cycle = mCycle + std::min(age, 3.0) * mCps;
   std::vector<int> visible;
   for (int i = 0; i < int(mLanes.size()); ++i)
      if (mLaneId.empty() || mLanes[i].id == mLaneId)
         visible.push_back(i);
   mRows.clear();
   mRowButtons.clear();
   mSourceButtons.clear();
   bool allOpen = !visible.empty();
   bool hasParts = false;
   for (int index : visible)
   {
      const auto& lane = mLanes[index];
      const bool open = mExpanded.count(lane.id) && !lane.parts.empty();
      if (!lane.parts.empty()) { hasParts = true; allOpen &= open; }
      auto expression = juce::String(lane.source).trim();
      auto prefix = expression.upToFirstOccurrenceOf(":", false, false);
      if (expression.contains(":") && prefix.containsOnly("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$"))
         expression = expression.fromFirstOccurrenceOf(":", false, false).trim();
      auto name = RustelSourceNavigation::SingleLine(expression.toStdString());
      if (name.empty()) name = lane.label;
      mRows.push_back({ index, "", (lane.parts.empty() ? "  " : open ? "[-] " : "[+] ") + lane.id + " / " + name, false });
      if (open)
         for (const auto& part : lane.parts)
            if (mRows.size() < 320) mRows.push_back({ index, part.name, "  > " + part.name, true });
   }
   mShowParts = hasParts && allOpen;
   const int count = int(mRows.size());
   ofSetColor(live ? ofColor(80, 235, 200) : ofColor(150, 160, 175));
   DrawTextNormal((mViewMode >= 5 && mViewMode <= 7) ? "MASTER AUDIO ANALYSIS" : (live ? "LIVE PATTERN FIELD" : "WAITING FOR RUSTEL"), 12, 70);
   ofSetColor(155, 175, 200);
   DrawTextNormal(ofToString(mCps * 240, 0) + " bpm  /  " + ofToString(count) + " lanes  /  cycle " + ofToString(cycle, 2), mWidth - 330, 70);
   const float sourceHeight = std::clamp(mHeight * 0.35f, 140.0f, 300.0f);
   const float top = 82, bottom = mHeight - sourceHeight - 50, fieldHeight = bottom - top;
   if (mViewMode == 0)
   {
      const float centerZ = std::max(0, count - 1) * 0.85f;
      const float yaw = mYaw + (mOrbit ? std::sin(float(mOrbitPhase)) * 0.6f : 0.0f);
      const float scale = std::min((mWidth - 60) / (19.0f + count * 0.35f), fieldHeight / (7.0f + count * 0.7f)) * mZoom;
      auto project = [&](Point p) -> Screen
      {
         float z = p.z - centerZ;
         float x = std::cos(yaw) * p.x + std::sin(yaw) * z;
         float depth = -std::sin(yaw) * p.x + std::cos(yaw) * z;
         float y = std::cos(mTilt) * p.y - std::sin(mTilt) * depth;
         depth = std::sin(mTilt) * p.y + std::cos(mTilt) * depth;
         float perspective = 30.0f / std::max(8.0f, 30.0f + depth);
         return { mWidth * (0.5f + mPanX) + x * scale * perspective, top + fieldHeight * (0.65f + mPanY) - y * scale * perspective, depth };
      };
      auto line = [&](Point a, Point b, ofColor color, float width)
      {
         auto p = project(a), q = project(b);
         ofSetColor(color);
         ofSetLineWidth(width);
         ofLine(p.x, p.y, q.x, q.y);
      };
      ofPushStyle();
      ofPushMatrix();
      ofClipWindow(2, top, mWidth - 4, fieldHeight, true);
      const int selectedRow = SelectedRow();
      if (selectedRow >= 0)
      {
         const float z = selectedRow * 1.7f;
         const ofColor selection(255, 215, 90, 225);
         const Point a{ -9, -0.08f, z - 0.55f }, b{ 6, -0.08f, z - 0.55f };
         const Point c{ 6, -0.08f, z + 0.55f }, d{ -9, -0.08f, z + 0.55f };
         line(a, b, selection, 1.8f); line(b, c, selection, 1.8f);
         line(c, d, selection, 1.8f); line(d, a, selection, 1.8f);
      }
      for (int beat = -8; beat <= 8; ++beat)
      {
         double b = std::floor(cycle * 4) + beat;
         float x = float(b / 4 - cycle) * 5;
         line({ x, -0.1f, -0.5f }, { x, -0.1f, centerZ * 2 + 0.5f }, ofColor(35, 55, 80, int64_t(b) % 4 == 0 ? 150 : 65), int64_t(b) % 4 == 0 ? 1.2f : 0.6f);
      }
      struct PartLabel { Screen point; std::string name; ofColor color; bool fitWidth{ false }; };
      std::vector<PartLabel> labels;
      std::vector<Face> faces;
      faces.reserve(std::min<size_t>(mEvents.size() * 12, 12000));
      auto box = [&](Point p, Point size, ofColor color, bool ghost, const std::string* note = nullptr)
      {
         std::array<Screen, 8> corners;
         for (int i = 0; i < 8; ++i)
            corners[i] = project({ p.x + ((i & 1) ? size.x : 0), p.y + ((i & 2) ? size.y : 0), p.z + ((i & 4) ? size.z : 0) });
         static const int indices[6][4] = { { 0, 1, 3, 2 }, { 4, 6, 7, 5 }, { 2, 3, 7, 6 }, { 0, 4, 5, 1 }, { 0, 2, 6, 4 }, { 1, 5, 7, 3 } };
         for (int f = 0; f < 6; ++f)
         {
            Face face;
            face.depth = 0;
            for (int i = 0; i < 4; ++i)
            {
               face.points[i] = corners[indices[f][i]];
               face.depth += face.points[i].depth * 0.25f;
            }
            // The corner lists wind inward; this camera looks along positive
            // depth, so negative screen area faces it. Skip hidden/edge-on sides.
            const float area = (face.points[1].x - face.points[0].x) * (face.points[2].y - face.points[0].y)
                             - (face.points[1].y - face.points[0].y) * (face.points[2].x - face.points[0].x);
            if (!ghost && area >= -0.01f)
               continue;
            face.color = color * (ghost ? 0.45f : (f == 2 ? 1.0f : 0.68f));
            face.color.a = ghost ? 85 : 255;
            if (!ghost && note && !note->empty())
            {
               face.note = note;
               face.center = { p.x + size.x * 0.5f, p.y + size.y * 0.5f, p.z + size.z * 0.5f };
               float width = size.x, height = size.y;
               if (f == 0) { face.center.z = p.z; face.right = { 1, 0, 0 }; face.down = { 0, -1, 0 }; }
               if (f == 1) { face.center.z = p.z + size.z; face.right = { -1, 0, 0 }; face.down = { 0, -1, 0 }; }
               if (f == 2) { face.center.y = p.y + size.y; face.right = { 1, 0, 0 }; face.down = { 0, 0, -1 }; height = size.z; }
               if (f == 3) { face.center.y = p.y; face.right = { 1, 0, 0 }; face.down = { 0, 0, 1 }; height = size.z; }
               if (f == 4) { face.center.x = p.x; face.right = { 0, 0, -1 }; face.down = { 0, -1, 0 }; width = size.z; }
               if (f == 5) { face.center.x = p.x + size.x; face.right = { 0, 0, 1 }; face.down = { 0, -1, 0 }; width = size.z; }
               face.textWidth = std::max(1.0f, gFontFixedWidth.GetStringWidth(*note, 12));
               face.textHeight = std::max(1.0f, gFontFixedWidth.GetStringHeight(*note, 12));
               face.textScale = std::min(width * 0.8f / face.textWidth, height * 0.65f / face.textHeight);
            }
            faces.push_back(face);
         }
      };
      int totalRendered = 0;
      for (int row = 0; row < count; ++row)
      {
         int lane = mRows[row].lane;
         float z = row * 1.7f;
         auto color = RowColor(row);
         const auto heading = project({ -8.7f, 0.25f, z });
         labels.push_back({ heading, mRows[row].label, color, true });
         if (!mRows[row].child && !mLanes[lane].parts.empty())
            mRowButtons.push_back({ ofRectangle(heading.x - 3, heading.y - 12, 24, 18), mLanes[lane].id });
         if (heading.y >= top && heading.y <= bottom)
         {
            const float inset = !mRows[row].child && !mLanes[lane].parts.empty() ? 24 : 0;
            const float captionWidth = std::min(gFont.GetStringWidth(mRows[row].label, 10), std::max(1.0f, mWidth - heading.x - 12));
            mSourceButtons.push_back({ ofRectangle(heading.x + inset, heading.y - 12, std::max(12.0f, captionWidth - inset), 18), mLanes[lane].id, mRows[row].part });
         }
         line({ -9, 0, z }, { 6, 0, z }, WithAlpha(color, 80), 1.4f);
         float pulse = 0;
         int rendered = 0;
         if (live)
            for (const auto& event : mEvents)
            {
               if (EventRow(event) != row || event.cycle + event.duration < cycle - 1.5 || event.cycle > cycle + 2)
                  continue;
               float elapsed = float((cycle - event.cycle) / mCps);
               float hit = elapsed >= 0 ? std::exp(-elapsed * 7) : 0;
               pulse = std::max(pulse, hit);
               if (rendered++ >= 128 || totalRendered >= 1024)
                  continue;
               ++totalRendered;
               float x = float(event.cycle - cycle) * 5;
               float length = float(std::clamp(event.duration, 0.012, 3.0)) * 5;
               auto eventColor = event.part.empty() ? color : ofColor(event.partColor[0], event.partColor[1], event.partColor[2]);
               if (!event.part.empty())
               {
                  const auto point = project({ x + length * 0.5f, (event.pitch - 24) * 0.065f + 0.55f + hit * 0.35f, z + 0.1f });
                  labels.push_back({ point, event.part, eventColor });
                  if (point.y >= top && point.y <= bottom)
                     mSourceButtons.push_back({ ofRectangle(point.x, point.y - 12, float(event.part.size() * 6), 18), mLanes[lane].id, event.part });
               }
               if (mTraces)
                  box({ x, (event.original - 24) * 0.065f + 0.3f, z - 0.25f }, { length, 0.035f, 0.055f }, eventColor, true);
               box({ x, (event.pitch - 24) * 0.065f + 0.3f + hit * 0.35f, z + 0.1f }, { length, 0.12f + hit * 0.2f, 0.22f }, eventColor, false, &event.note);
            }
         const float radius = 0.3f + pulse * 0.65f;
         for (int segment = 0; segment < 32; ++segment)
         {
            float a = segment * 6.2831853f / 32, b = (segment + 1) * 6.2831853f / 32;
            line({ std::cos(a) * radius, 0.02f, z + std::sin(a) * radius }, { std::cos(b) * radius, 0.02f, z + std::sin(b) * radius }, WithAlpha(color, int(70 + pulse * 150)), 1.2f);
         }
         auto lamp = project({ 0, 0.15f, z });
         for (int glow = 3; glow >= 1; --glow)
         {
            ofFill();
            ofSetColor(color, (25 + pulse * 40) / glow);
            ofCircle(lamp.x, lamp.y, (3 + pulse * 5) * glow);
         }
      }
      std::sort(faces.begin(), faces.end(), [](const Face& a, const Face& b)
                {
                   return a.depth > b.depth;
                });
      for (const auto& face : faces)
      {
         Polygon(face.points, face.color);
         if (!face.note || face.textScale <= 0)
            continue;
         // Draw each glyph in its face's local tangent frame. Evaluating the
         // projection per character follows perspective across longer labels.
         float offset = -face.textWidth * 0.5f;
         for (char character : *face.note)
         {
            const std::string glyph(1, character);
            const float baseline = face.textHeight * 0.35f;
            auto sample = [&](float u, float v)
            {
               return project({ face.center.x + (face.right.x * u + face.down.x * v) * face.textScale,
                                face.center.y + (face.right.y * u + face.down.y * v) * face.textScale,
                                face.center.z + (face.right.z * u + face.down.z * v) * face.textScale });
            };
            const auto origin = sample(offset, baseline);
            const auto u = sample(offset + 1, baseline), v = sample(offset, baseline + 1);
            const float ux = u.x - origin.x, uy = u.y - origin.y;
            const float vx = v.x - origin.x, vy = v.y - origin.y;
            if (std::abs(ux * vy - uy * vx) > 0.00001f)
            {
               ofPushMatrix();
               nvgTransform(gNanoVG, ux, uy, vx, vy, origin.x, origin.y);
               ofSetColor(5, 9, 18);
               gFontFixedWidth.DrawString(glyph, 12, 0, 0);
               ofPopMatrix();
            }
            offset += gFontFixedWidth.GetStringWidth(glyph, 12);
         }
      }
      line({ 0, 0, -0.6f }, { 0, 0, centerZ * 2 + 0.6f }, ofColor(190, 225, 255, 200), 2);
      for (const auto& label : labels)
      {
         ofSetColor(label.color);
         const float width = label.fitWidth ? gFont.GetStringWidth(label.name, 10) : 0;
         const float fontSize = width > 0 ? 10 * std::min(1.0f, std::max(1.0f, mWidth - label.point.x - 12) / width) : 10;
         DrawTextNormal(label.name, label.point.x, label.point.y, fontSize);
      }
      ofPopMatrix(); // Restore the parent's module clip before drawing footer.
      ofPopStyle();
   }
   else if (mViewMode < 5)
   {
      DrawEventView(visible, cycle, live, top, bottom);
   }
   else if (mViewMode == 8)
   {
      DrawControls(visible, cycle, live, top, bottom);
   }
   else
   {
      DrawAudioView(top, bottom, dt);
   }
   ofSetColor(110, 135, 160);
   DrawTextNormal((mViewMode >= 5 && mViewMode <= 7) ? "master tap (pre gain/limiter) / lane selection does not isolate audio / display gain is visual only" : (live ? (mViewMode == 0 ? "drag: orbit / right or shift-drag: pan / wheel: zoom / code panel: scroll" : "drag: pan / wheel: zoom / code panel: scroll") : mStatus + " / start Rustel with RUSTEL_MUSIC_LINK=1"), 12, mHeight - 12, 10);
   for (int i = 0; i < std::min(count, 12); ++i)
   {
      int lane = mRows[i].lane;
      float x = 12 + i * (mWidth - 24) / std::max(1, std::min(count, 12));
      ofSetColor(RowColor(i));
      DrawTextNormal(mRows[i].child ? mRows[i].part : mLanes[lane].id, x, bottom + 18, 9);
      const float width = (mWidth - 24) / std::max(1, std::min(count, 12));
      mSourceButtons.push_back({ ofRectangle(x, bottom + 5, width, 20), mLanes[lane].id, mRows[i].part });
   }
   const float sourceTop = bottom + 30;
   const float sourceBottom = mHeight - 28;
   const float sourceViewHeight = sourceBottom - sourceTop;
   mSourceScroll = std::clamp(mSourceScroll, 0.0f, std::max(0.0f, mSourceContentHeight - sourceViewHeight));
   ofFill();
   ofSetColor(10, 17, 28);
   ofRect(6, sourceTop, mWidth - 12, sourceViewHeight, 4);
   ofPushMatrix();
   ofClipWindow(8, sourceTop, mWidth - 16, sourceViewHeight, true);
   float cursor = sourceTop + 18 - mSourceScroll;
   if (!mDocument.empty())
      cursor = DrawSourceDocument(visible, cycle, live, cursor);
   else for (int lane : visible)
   {
      ofSetColor(kColors[lane % kColors.size()]);
      DrawTextNormal(mLanes[lane].id + " / line source", 16, cursor, 10);
      cursor += 16;
      ofSetColor(210, 224, 240);
      auto text = mLanes[lane].source.empty() ? "Source unavailable: restart Rustel with the updated bridge." : mLanes[lane].source;
      auto bounds = gFontFixedWidth.DrawStringWrap(text, 11, 16, cursor, mWidth - 42);
      cursor += std::max(16.0f, bounds.height) + 26;
   }
   ofPopMatrix();
   mSourceContentHeight = cursor - (sourceTop + 18 - mSourceScroll);
   if (mSourceContentHeight > sourceViewHeight)
   {
      float thumb = std::max(15.0f, sourceViewHeight * sourceViewHeight / mSourceContentHeight);
      float offset = (sourceViewHeight - thumb) * mSourceScroll / std::max(1.0f, mSourceContentHeight - sourceViewHeight);
      ofSetColor(85, 115, 145);
      ofRect(mWidth - 12, sourceTop + offset, 3, thumb, 1);
   }
   ofPopStyle();
}

bool RustelView::InNoteField(float x, float y)
{
   const float bottom = mHeight - std::clamp(mHeight * 0.35f, 140.0f, 300.0f) - 50;
   return !Minimized() && IsVisible() && x >= 2 && x <= mWidth - 2 && y >= 82 && y <= bottom;
}

void RustelView::OnClicked(float x, float y, bool right)
{
   IDrawableModule::OnClicked(x, y, right);
   if (!right && mViewMode == 0 && InNoteField(x, y))
      for (const auto& button : mRowButtons)
         if (button.first.contains(x, y))
         {
            if (mExpanded.count(button.second)) mExpanded.erase(button.second); else mExpanded.insert(button.second);
            return;
         }
   if (!right && y >= 82 && x >= 2 && x <= mWidth - 2)
      for (auto button = mSourceButtons.rbegin(); button != mSourceButtons.rend(); ++button)
         if (button->bounds.contains(x, y))
         {
            RevealSource(button->lane, button->part);
            return;
         }
   if (!InNoteField(x, y) || IsKeyHeld(' ') || (GetKeyModifiers() & kModifier_Alt))
      return;
   mCameraDragging = true;
   mPanDragging = mViewMode != 0 || right || (GetKeyModifiers() & kModifier_Shift);
   mDragButton = right ? 2 : 1;
   mDragX = x;
   mDragY = y;
   if (!mPanDragging && mOrbit)
   {
      mYawSlider->SetValue(std::remainder(mYaw + std::sin(float(mOrbitPhase)) * 0.6f, float(TWO_PI)), gTime);
      mOrbit = false;
   }
}

bool RustelView::MouseMoved(float x, float y)
{
   IDrawableModule::MouseMoved(x, y);
   if (!mCameraDragging)
      return false;
   // Bespoke dispatches moves to all modules, so a drag stays captured outside
   // the viewport; release or loss of the pressed button ends that capture.
   if (!TheSynth->IsMouseButtonHeld(mDragButton))
   {
      mCameraDragging = false;
      return false;
   }
   float dx = x - mDragX, dy = y - mDragY;
   mDragX = x;
   mDragY = y;
   if (mPanDragging)
   {
      const float height = mHeight - std::clamp(mHeight * 0.35f, 140.0f, 300.0f) - 132;
      mPanX = std::clamp(mPanX + dx / mWidth, -4.0f, 4.0f);
      mPanY = std::clamp(mPanY + dy / height, -4.0f, 4.0f);
   }
   else
   {
      mYawSlider->SetValue(std::remainder(mYaw + dx * 0.008f, float(TWO_PI)), gTime);
      mTiltSlider->SetValue(std::clamp(mTilt + dy * 0.008f, -1.45f, 1.45f), gTime);
   }
   return true;
}

void RustelView::MouseReleased()
{
   mCameraDragging = false;
   IDrawableModule::MouseReleased();
}

void RustelView::CheckboxUpdated(Checkbox* checkbox, double)
{
   if (checkbox == mPartsCheckbox)
   {
      for (const auto& lane : mLanes)
         if (mLaneId.empty() || lane.id == mLaneId)
         {
            if (mShowParts) mExpanded.insert(lane.id); else mExpanded.erase(lane.id);
         }
      return;
   }
   if (checkbox != mSoloCheckbox) return;
   const auto target = !mSolo && !mSoloLane.empty() ? mSoloLane : SoloTarget();
   if (target.empty() || !mLive) { mSolo = false; return; }
   RustelLink::Solo(mSessionId, mGeneration, target, mSolo);
}

void RustelView::ButtonClicked(ClickButton* button, double time)
{
   if (button != mResetCamera)
      return;
   mPanX = mPanY = 0;
   mOrbitPhase = 0;
   mCameraDragging = false;
   mOrbit = false;
   mYawSlider->SetValue(0.35f, time);
   mTiltSlider->SetValue(0.6f, time);
   mZoomSlider->SetValue(1, time);
}

bool RustelView::MouseScrolled(float x, float y, float, float scrollY, bool isSmoothScroll, bool)
{
   if (InNoteField(x, y))
   {
      const float height = mHeight - std::clamp(mHeight * 0.35f, 140.0f, 300.0f) - 132;
      float oldZoom = mZoom;
      float zoom = std::clamp(oldZoom * std::exp(std::clamp(scrollY * 0.08f, -1.0f, 1.0f)), 0.2f, 5.0f);
      // Keep the point under the cursor fixed while dollying the view.
      float ratio = zoom / oldZoom;
      mPanX = std::clamp(mPanX + (x - mWidth * (0.5f + mPanX)) * (1 - ratio) / mWidth, -4.0f, 4.0f);
      mPanY = std::clamp(mPanY + (y - 82 - height * (0.65f + mPanY)) * (1 - ratio) / height, -4.0f, 4.0f);
      mZoomSlider->SetValue(zoom, gTime);
      return true;
   }
   const float sourceTop = mHeight - std::clamp(mHeight * 0.35f, 140.0f, 300.0f) - 20;
   if (x < 6 || x > mWidth - 6 || y < sourceTop || y > mHeight - 28)
      return false;
   mSourceScroll = std::max(0.0f, mSourceScroll - scrollY * (isSmoothScroll ? 1.0f : 24.0f));
   return true;
}

void RustelView::SaveState(FileStreamOut& out)
{
   out << GetModuleSaveStateRev();
   IDrawableModule::SaveState(out);
   out << mSessionId;
   out << mLaneId;
   out << mPanX;
   out << mPanY;
}
void RustelView::LoadState(FileStreamIn& in, int rev)
{
   IDrawableModule::LoadState(in, rev);
   LoadStateValidate(rev <= GetModuleSaveStateRev());
   in >> mSessionId;
   in >> mLaneId;
   mPanX = mPanY = 0;
   if (rev >= 1)
   {
      in >> mPanX;
      in >> mPanY;
   }
   mCameraDragging = false;
   mSessionIds.clear();
   mSourceFocusFrom = mSourceFocusTo = -1;
   mSourceFocusName.clear(); mSelectedLane.clear(); mSelectedPart.clear();
   mSourceButtons.clear();
   mRowButtons.clear();
   mSoloLane.clear();
   mSolo = false;
   mViewMode = std::clamp(mViewMode, 0, 8);
   mLanes.clear();
   mEvents.clear();
   mGeneration = -1;
   Resize(mWidth, mHeight);
   mAudioSequence = 0;
   mAudioGeneration = -1;
   mAudioScope.clear(); mAudioSpectrum.clear(); mAudioSides.clear();
}


float RustelView::DrawSourceDocument(const std::vector<int>& visible, double cycle, bool live, float y)
{
   ofSetColor(155, 175, 200);
   DrawTextNormal(mSourceFocusName.empty() ? "evaluated document / sounding tokens highlighted" : "evaluated document / selected: " + mSourceFocusName, 16, y, 10);
   y += 18;
   const float charWidth = std::max(1.0f, gFontFixedWidth.GetStringWidth("M", 11));
   const int columns = std::max(10, int((mWidth - 42) / charWidth));
   size_t begin = 0;
   int rows = 0;
   bool selectionBox = false;
   float boxLeft = mWidth, boxRight = 0, boxTop = y, boxBottom = y;
   while (begin < mDocument.size() && rows++ < 8192)
   {
      size_t end = begin;
      int characters = 0;
      while (end < mDocument.size() && mDocument[end] != '\n' && characters < columns)
      {
         ++end;
         while (end < mDocument.size() && (static_cast<unsigned char>(mDocument[end]) & 0xc0) == 0x80) ++end;
         ++characters;
      }
      auto row = mDocument.substr(begin, end - begin);
      if (mSourceFocusFrom >= 0 && mSourceFocusTo > mSourceFocusFrom)
      {
         const size_t a = std::max(begin, size_t(mSourceFocusFrom));
         const size_t b = std::min(end, size_t(mSourceFocusTo));
         if (b > a)
         {
            const float x = 16 + gFontFixedWidth.GetStringWidth(mDocument.substr(begin, a - begin), 11);
            const float right = x + gFontFixedWidth.GetStringWidth(mDocument.substr(a, b - a), 11);
            if (!selectionBox) { boxTop = y - 14; selectionBox = true; }
            boxLeft = std::min(boxLeft, x - 3); boxRight = std::max(boxRight, right + 3); boxBottom = y + 5;
         }
      }
      if (live && y >= 82 && y <= mHeight)
         for (const auto& event : mEvents)
         {
            if (cycle < event.cycle || cycle >= event.cycle + event.duration || std::find(visible.begin(), visible.end(), event.lane) == visible.end()) continue;
            for (const auto& range : event.context)
            {
               size_t a = std::max(begin, size_t(std::max(0, range.first)));
               size_t b = std::min(end, size_t(std::max(0, range.second)));
               if (b <= a) continue;
               float x = 16 + gFontFixedWidth.GetStringWidth(mDocument.substr(begin, a - begin), 11);
               float width = gFontFixedWidth.GetStringWidth(mDocument.substr(a, b - a), 11);
               ofFill();
               auto tint = event.part.empty() ? kColors[event.lane % kColors.size()] : ofColor(event.partColor[0], event.partColor[1], event.partColor[2]);
               ofSetColor(tint, 85);
               ofRect(x, y - 12, width, 16);
            }
         }
      if (y >= 82 && y <= mHeight)
      {
         ofSetColor(210, 224, 240);
         gFontFixedWidth.DrawString(row, 11, 16, y);
         for (const auto& binding : mBindings)
         {
            size_t a = std::max(begin, size_t(std::max(0, binding.from)));
            size_t b = std::min(end, size_t(std::max(0, binding.to)));
            if (b <= a) continue;
            ofSetColor(binding.color);
            float x = 16 + gFontFixedWidth.GetStringWidth(mDocument.substr(begin, a - begin), 11);
            gFontFixedWidth.DrawString(mDocument.substr(a, b - a), 11, x, y);
         }
      }
      y += 16;
      begin = end;
      if (begin < mDocument.size() && mDocument[begin] == '\n') ++begin;
   }
   if (selectionBox)
   {
      ofPushStyle(); ofNoFill(); ofSetColor(255, 215, 90); ofSetLineWidth(1.5f);
      ofRect(boxLeft, boxTop, boxRight - boxLeft, boxBottom - boxTop, 2);
      ofPopStyle();
   }
   return y + 16;
}

void RustelView::DrawControls(const std::vector<int>& visible, double cycle, bool live, float top, float bottom)
{
   const char* keys[] = { "gain", "pan", "cutoff (Hz, log)", "resonance" };
   const float left = 125, right = mWidth - 20;
   const float height = (bottom - top) / 4;
   auto xAt = [&](double at) { return left + float((at - cycle) / (2.0 / mZoom) + 0.75 + mPanX) * (right - left); };
   ofPushMatrix();
   ofClipWindow(2, top, mWidth - 4, bottom - top, true);
   for (int control = 0; control < 4; ++control)
   {
      const float y = top + control * height;
      double low = 0, high = 1;
      for (const auto& event : mEvents)
         if (event.hasControl[control] && std::find(visible.begin(), visible.end(), event.lane) != visible.end())
         {
            double value = control == 2 ? std::log10(std::max(1.0, event.controls[control])) : event.controls[control];
            low = std::min(low, value); high = std::max(high, value);
         }
      ofSetColor(150, 175, 200);
      DrawTextNormal(keys[control], 10, y + 18, 10);
      DrawTextNormal("onset samples", 10, y + 34, 9);
      ofSetColor(40, 60, 80);
      ofLine(left, y + height - 8, right, y + height - 8);
      ofSetColor(180, 210, 235, 100);
      ofLine(xAt(cycle), y + 4, xAt(cycle), y + height - 8);
      if (!live) continue;
      for (int row = 0; row < int(mRows.size()); ++row)
      {
         std::vector<const Event*> samples;
         for (const auto& event : mEvents)
            if (EventRow(event) == row && event.hasControl[control]) samples.push_back(&event);
         std::stable_sort(samples.begin(), samples.end(), [](const Event* a, const Event* b) { return a->cycle < b->cycle; });
         bool previous = false;
         float px = 0, py = 0;
         ofSetColor(RowColor(row));
         for (const auto* event : samples)
         {
            double value = control == 2 ? std::log10(std::max(1.0, event->controls[control])) : event->controls[control];
            float x = xAt(event->cycle), ey = y + height - 12 - float((value - low) / (high - low)) * std::max(1.0f, height - 22);
            if (previous) { ofLine(px, py, x, py); ofLine(x, py, x, ey); }
            ofFill(); ofCircle(x, ey, 2.5f);
            px = x; py = ey; previous = true;
         }
      }
   }
   ofPopMatrix();
}
