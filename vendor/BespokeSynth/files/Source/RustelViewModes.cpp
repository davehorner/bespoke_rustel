#include "RustelView.h"
#include "OpenFrameworksPort.h"
#include <algorithm>
#include <array>
#include <cmath>

// Native event-driven views. Audio scopes need a separate analysis feed.
void RustelView::DrawEventView(const std::vector<int>& visible, double cycle, bool live, float top, float bottom)
{
   static const std::array<ofColor, 8> colors{ { { 31, 242, 209 }, { 255, 89, 107 }, { 163, 107, 255 }, { 255, 186, 51 }, { 51, 166, 255 }, { 255, 89, 204 }, { 158, 255, 71 }, { 255, 138, 56 } } };
   static const char* notes[12] = { "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B" };
   const float width = mWidth - 76, height = bottom - top - 26, left = 52, edge = top + 12;
   float low = 127, high = 0;
   for (const auto& event : mEvents)
   {
      if (std::find(visible.begin(), visible.end(), event.lane) == visible.end())
         continue;
      if (event.cycle + event.duration < cycle - 1.5 || event.cycle > cycle + 0.5)
         continue;
      low = std::min(low, std::min(event.pitch, mTraces ? event.original : event.pitch));
      high = std::max(high, std::max(event.pitch, mTraces ? event.original : event.pitch));
   }
   if (low > high)
   {
      low = 36;
      high = 84;
   }
   low = std::clamp(std::floor(low / 12) * 12, -24.0f, 120.0f);
   high = std::clamp(std::max(low + 24, std::ceil((high + 1) / 12) * 12), low + 24, 144.0f);
   auto transform = [&](float x, float y)
   {
      const float cx = mWidth * 0.5f, cy = top + (bottom - top) * 0.65f;
      return ofVec2f(cx + (x - cx) * mZoom + mPanX * mWidth, cy + (y - cy) * mZoom + mPanY * (bottom - top));
   };
   auto line = [&](float x, float y, float a, float b, ofColor color, float thickness = 1.0f)
   {
      auto p = transform(x, y), q = transform(a, b);
      ofSetColor(color);
      ofSetLineWidth(thickness);
      ofLine(p.x, p.y, q.x, q.y);
   };
   auto circle = [&](float x, float y, float radius, ofColor color, bool filled)
   {
      auto p = transform(x, y);
      ofSetColor(color);
      ofSetLineWidth(1.2f);
      if (filled)
         ofFill();
      else
         ofNoFill();
      ofCircle(p.x, p.y, radius * mZoom);
   };
   auto text = [&](const std::string& label, float x, float y, ofColor color)
   {
      auto p = transform(x, y);
      ofSetColor(color);
      DrawTextNormal(label, p.x, p.y, 10);
   };
   auto rect = [&](float x, float y, float w, float h, ofColor color)
   {
      auto p = transform(x, y);
      ofFill();
      ofSetColor(color);
      ofRect(p.x, p.y, w * mZoom, h * mZoom, 1);
   };
   auto pitchX = [&](float p)
   {
      return left + (p - low) / (high - low) * width;
   };
   auto pitchY = [&](float p)
   {
      return edge + height - (p - low) / (high - low) * height;
   };
   auto timeX = [&](double t)
   {
      return left + float(t - cycle + 1.5) / 2 * width;
   };
   auto timeY = [&](double t)
   {
      return edge + float(0.5 - (t - cycle)) / 2 * height;
   };
   const float cx = left + width * 0.5f, cy = edge + height * 0.5f, radius = std::min(width, height) * 0.43f;
   auto spiral = [&](double t, float p)
   {
      float age = float(t - cycle);
      float r = radius * (0.12f + (age + 1.5f) * 0.39f + (p - low) / (high - low) * 0.08f);
      float angle = age * TWO_PI - PI * 0.5f;
      return ofVec2f(cx + std::cos(angle) * r, cy + std::sin(angle) * r);
   };
   auto wheel = [&](float p)
   {
      float angle = std::fmod(p, 12.0f) / 12 * TWO_PI - PI * 0.5f;
      float r = radius * (0.3f + 0.65f * (p - low) / (high - low));
      return ofVec2f(cx + std::cos(angle) * r, cy + std::sin(angle) * r);
   };
   ofPushStyle();
   ofPushMatrix();
   ofClipWindow(2, top, mWidth - 4, bottom - top, true);
   if (mViewMode <= 2)
   {
      for (int pitch = int(low); pitch <= int(high); ++pitch)
      {
         bool octave = pitch % 12 == 0;
         auto color = ofColor(35, 55, 78, octave ? 170 : 65);
         if (mViewMode == 1)
         {
            float y = pitchY(float(pitch));
            line(left, y, left + width, y, color);
            if (octave)
               text(std::string("C") + ofToString(pitch / 12 - 1), left - 30, y + 3, ofColor(120, 150, 175));
         }
         else
         {
            float x = pitchX(float(pitch));
            line(x, edge, x, edge + height, color);
            if (octave)
               text(std::string("C") + ofToString(pitch / 12 - 1), x, edge + height + 12, ofColor(120, 150, 175));
         }
      }
      for (int beat = -6; beat <= 2; ++beat)
      {
         double t = (std::floor(cycle * 4) + beat) / 4;
         if (mViewMode == 1)
            line(timeX(t), edge, timeX(t), edge + height, ofColor(40, 65, 85, 100));
         else
            line(left, timeY(t), left + width, timeY(t), ofColor(40, 65, 85, 100));
      }
   }
   else if (mViewMode == 3)
   {
      auto previous = spiral(cycle - 1.5, low);
      for (int i = 1; i <= 160; ++i)
      {
         auto next = spiral(cycle - 1.5 + i / 80.0, low);
         line(previous.x, previous.y, next.x, next.y, ofColor(55, 80, 110, 140));
         previous = next;
      }
      auto head = spiral(cycle, low);
      circle(head.x, head.y, 5, ofColor(210, 240, 255), true);
   }
   else
   {
      for (int ring = 0; ring < 4; ++ring)
         circle(cx, cy, radius * (0.3f + ring * 0.22f), ofColor(40, 65, 90, 170), false);
      for (int pitch = 0; pitch < 12; ++pitch)
      {
         float a = pitch * TWO_PI / 12 - PI * 0.5f;
         line(cx, cy, cx + std::cos(a) * radius, cy + std::sin(a) * radius, ofColor(40, 65, 90, 120));
         text(notes[pitch], cx + std::cos(a) * (radius + 15) - 5, cy + std::sin(a) * (radius + 15) + 3, ofColor(145, 175, 200));
      }
   }
   int rendered = 0;
   if (live)
      for (const auto& event : mEvents)
      {
         if (std::find(visible.begin(), visible.end(), event.lane) == visible.end() || event.cycle + event.duration < cycle - 1.5 || event.cycle > cycle + 0.5)
            continue;
         if (rendered++ >= 1024)
            break;
         bool sounding = event.cycle <= cycle && event.cycle + event.duration > cycle;
         float elapsed = float((cycle - event.cycle) / mCps);
         float hit = elapsed >= 0 ? std::exp(-elapsed * 7) : 0;
         for (bool source : { true, false })
         {
            if (source && !mTraces)
               continue;
            float pitch = source ? event.original : event.pitch;
            auto color = colors[event.lane % colors.size()];
            color.a = source ? 65 : (sounding ? 255 : 150);
            if (mViewMode == 1)
            {
               float h = std::max(2.0f, height / (high - low) * 0.75f);
               rect(timeX(event.cycle), pitchY(pitch) - h * 0.5f, std::max(2.0f, float(event.duration) * width / 2), source ? 1.5f : h, color);
            }
            else if (mViewMode == 2)
            {
               float w = std::max(3.0f, width / (high - low) * 0.65f);
               rect(pitchX(pitch) - w * 0.5f, timeY(event.cycle + event.duration), source ? 1.5f : w, std::max(3.0f, float(event.duration) * height / 2), color);
            }
            else if (mViewMode == 3)
            {
               auto p = spiral(event.cycle, pitch);
               auto previous = p;
               double duration = std::clamp(event.duration, 0.01, 0.5);
               for (int i = 1; i <= 12; ++i)
               {
                  auto next = spiral(event.cycle + duration * i / 12, pitch);
                  line(previous.x, previous.y, next.x, next.y, color, source ? 0.7f : 2.0f);
                  previous = next;
               }
               circle(p.x, p.y, source ? 2 : 3 + hit * 5, color, !source);
            }
            else
            {
               auto p = wheel(pitch);
               if (sounding && !source)
                  line(cx, cy, p.x, p.y, color, 1.5f);
               circle(p.x, p.y, source ? 3 : 3 + hit * 7 + event.gain * 2, color, !source);
            }
         }
      }
   if (mViewMode == 1)
      line(timeX(cycle), edge, timeX(cycle), edge + height, ofColor(210, 240, 255, 220), 1.5f);
   if (mViewMode == 2)
      line(left, timeY(cycle), left + width, timeY(cycle), ofColor(210, 240, 255, 220), 1.5f);
   ofPopMatrix();
   ofPopStyle();
}
