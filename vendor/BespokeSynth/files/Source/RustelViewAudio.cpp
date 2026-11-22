#include "RustelView.h"
#include "RustelLink.h"
#include "OpenFrameworksPort.h"
#include <algorithm>
#include <cmath>

void RustelView::PollAudio()
{
   auto frames = RustelLink::Analysis();
   auto* list = frames.getArray();
   if (!list)
      return;
   for (const auto& frame : *list)
   {
      if (frame["session"].toString().toStdString() != mSessionId || int(frame["version"]) != 1)
         continue;
      uint64_t sequence = int64_t(frame["sequence"]);
      if (sequence <= mAudioSequence)
         continue;
      auto* scope = frame["scope"].getArray();
      auto* spectrum = frame["spectrum"].getArray();
      auto* sides = frame["sides"].getArray();
      int rate = int(frame["sample_rate"]);
      if (!scope || !spectrum || !sides || scope->size() != 512 || spectrum->size() != 512 || sides->size() > 4096 || rate <= 0)
         continue;
      std::vector<float> wave, bins;
      std::vector<ofVec2f> stereo;
      bool valid = true;
      for (const auto& value : *scope)
      {
         float v = float(value);
         if (!std::isfinite(v))
         {
            valid = false;
            break;
         }
         wave.push_back(v);
      }
      for (const auto& value : *spectrum)
      {
         float v = float(value);
         if (!std::isfinite(v))
         {
            valid = false;
            break;
         }
         bins.push_back(std::clamp(v, -120.0f, 12.0f));
      }
      for (const auto& pair : *sides)
      {
         auto* values = pair.getArray();
         if (!values || values->size() != 2)
         {
            valid = false;
            break;
         }
         float l = float((*values)[0]), r = float((*values)[1]);
         if (!std::isfinite(l) || !std::isfinite(r))
         {
            valid = false;
            break;
         }
         stereo.emplace_back(l, r);
      }
      if (!valid)
         continue;
      uint64_t stream = int64_t(frame["stream_id"]), epoch = int64_t(frame["epoch"]);
      int64_t generation = int64_t(frame["generation"]);
      if (stream != mAudioStream || epoch != mAudioEpoch || generation != mAudioGeneration)
      {
         mSpectrumBars.fill(0);
         mSpectrumPeaks.fill(0);
      }
      mAudioSequence = sequence;
      mAudioStream = stream;
      mAudioEpoch = epoch;
      mAudioGeneration = generation;
      mAudioSampleRate = rate;
      mAudioScope = std::move(wave);
      mAudioSpectrum = std::move(bins);
      mAudioSides = std::move(stereo);
      mAudioReceived = juce::Time::getMillisecondCounterHiRes();
   }
}

void RustelView::DrawAudioView(float top, float bottom, double dt)
{
   ofPushStyle();
   ofPushMatrix();
   ofClipWindow(2, top, mWidth - 4, bottom - top, true);
   const float left = 46, right = mWidth - 20, width = right - left, edge = top + 20, height = bottom - edge - 24;
   auto transform = [&](float x, float y)
   {
      float cx = mWidth * 0.5f, cy = top + (bottom - top) * 0.65f;
      return ofVec2f(cx + (x - cx) * mZoom + mPanX * mWidth, cy + (y - cy) * mZoom + mPanY * (bottom - top));
   };
   auto line = [&](float x, float y, float a, float b, ofColor c, float w = 1.0f)
   {
      auto p = transform(x, y), q = transform(a, b);
      ofSetColor(c);
      ofSetLineWidth(w);
      ofLine(p.x, p.y, q.x, q.y);
   };
   auto text = [&](std::string s, float x, float y, ofColor c)
   {
      auto p = transform(x, y);
      ofSetColor(c);
      DrawTextNormal(s, p.x, p.y, 10);
   };
   bool fresh = juce::Time::getMillisecondCounterHiRes() - mAudioReceived < 1000 && mAudioGeneration == mGeneration && mAudioSampleRate > 0;
   if (!fresh)
   {
      ofSetColor(140, 165, 185);
      DrawTextNormal("Waiting for live master audio from Rustel...", 20, int(top + 40));
      ofPopMatrix();
      ofPopStyle();
      return;
   }
   if (mViewMode == 5)
   {
      for (int i = 0; i < 5; ++i)
      {
         float y = edge + height * i / 4;
         line(left, y, right, y, ofColor(40, 60, 85, 140));
         text(ofToString(1.0 - i * 0.5, 1), 8, y + 3, ofColor(110, 140, 170));
      }
      for (int i = 0; i < 9; ++i)
      {
         float x = left + width * i / 8;
         line(x, edge, x, edge + height, ofColor(35, 55, 75, 100));
      }
      size_t start = 0;
      for (size_t i = 1; i + 256 < mAudioScope.size(); ++i)
      {
         if (mAudioScope[i - 1] <= 0 && mAudioScope[i] > 0)
         {
            start = i;
            break;
         }
      }
      size_t count = std::min<size_t>(256, mAudioScope.size() - start);
      for (size_t i = 1; i < count; ++i)
      {
         float x = left + width * float(i - 1) / float(count - 1), nx = left + width * float(i) / float(count - 1);
         float y = edge + height * 0.5f - mAudioScope[start + i - 1] * mDisplayGain * height * 0.5f;
         float ny = edge + height * 0.5f - mAudioScope[start + i] * mDisplayGain * height * 0.5f;
         line(x, y, nx, ny, ofColor(35, 220, 190, 45), 5);
         line(x, y, nx, ny, ofColor(100, 255, 220), 1.5f);
      }
      text("rising-edge trigger / display x" + ofToString(mDisplayGain, 1), left, edge + height + 15, ofColor(120, 165, 180));
   }
   else if (mViewMode == 6)
   {
      const float nyquist = mAudioSampleRate * 0.5f, minimum = 20, maximum = std::min(20000.0f, nyquist);
      for (int db = -90; db <= 0; db += 30)
      {
         float y = edge + height * (1 - (db + 90) / 90.0f);
         line(left, y, right, y, ofColor(40, 60, 85, 140));
         text(ofToString(db), 8, y + 3, ofColor(110, 140, 170));
      }
      float gainDb = 20 * std::log10(mDisplayGain);
      float smooth = 1 - std::exp(-float(dt) * 18);
      for (int i = 0; i < 64; ++i)
      {
         float low = minimum * std::pow(maximum / minimum, i / 64.0f), high = minimum * std::pow(maximum / minimum, (i + 1) / 64.0f);
         int a = std::clamp(int(low / nyquist * 512), 0, 511), b = std::clamp(int(std::ceil(high / nyquist * 512)), a + 1, 512);
         float db = -120;
         for (int bin = a; bin < b; ++bin)
            db = std::max(db, mAudioSpectrum[bin]);
         float level = std::clamp((db + gainDb + 90) / 90, 0.0f, 1.0f);
         mSpectrumBars[i] += (level - mSpectrumBars[i]) * smooth;
         mSpectrumPeaks[i] = std::max(mSpectrumBars[i], mSpectrumPeaks[i] - float(dt) * 0.2f);
         auto p = transform(left + i * width / 64, edge + height * (1 - mSpectrumBars[i]));
         ofFill();
         ofSetColor(40 + i * 2, 220 - i, 210);
         ofRect(p.x, p.y, std::max(1.0f, (width / 64 - 2) * mZoom), height * mSpectrumBars[i] * mZoom, 1);
         line(left + i * width / 64, edge + height * (1 - mSpectrumPeaks[i]), left + (i + 1) * width / 64 - 2, edge + height * (1 - mSpectrumPeaks[i]), ofColor(215, 250, 255), 1);
      }
      for (float hz : { 20.0f, 100.0f, 1000.0f, 10000.0f })
         if (hz < maximum)
         {
            float x = left + std::log(hz / minimum) / std::log(maximum / minimum) * width;
            text(hz >= 1000 ? ofToString(hz / 1000, 0) + "k" : ofToString(hz, 0), x, edge + height + 15, ofColor(130, 165, 185));
         }
   }
   else
   {
      if (mAudioSides.size() < 2)
      {
         text("No stereo analysis available", left, edge + 30, ofColor(145, 165, 190));
         ofPopMatrix();
         ofPopStyle();
         return;
      }
      float cx = left + width * 0.5f, cy = edge + height * 0.5f, radius = height * 0.44f;
      line(cx - radius, cy, cx + radius, cy, ofColor(55, 80, 105));
      line(cx, cy - radius, cx, cy + radius, ofColor(55, 80, 105));
      line(cx - radius, cy, cx, cy - radius, ofColor(35, 60, 85));
      line(cx, cy - radius, cx + radius, cy, ofColor(35, 60, 85));
      line(cx + radius, cy, cx, cy + radius, ofColor(35, 60, 85));
      line(cx, cy + radius, cx - radius, cy, ofColor(35, 60, 85));
      double ll = 0, rr = 0, lr = 0;
      ofVec2f previous;
      bool first = true;
      for (size_t i = 0; i < mAudioSides.size(); ++i)
      {
         float l = mAudioSides[i].x, r = mAudioSides[i].y;
         ll += l * l;
         rr += r * r;
         lr += l * r;
         float side = (r - l) * 0.70710678f, mid = (l + r) * 0.70710678f;
         ofVec2f point(cx + side * mDisplayGain * radius, cy - mid * mDisplayGain * radius);
         if (!first)
            line(previous.x, previous.y, point.x, point.y, ofColor(80, 235, 215, 35 + int(190 * i / mAudioSides.size())), 1.2f);
         previous = point;
         first = false;
      }
      double correlation = ll > 1e-12 && rr > 1e-12 ? std::clamp(lr / std::sqrt(ll * rr), -1.0, 1.0) : 0;
      text("MID", cx - 10, edge + 3, ofColor(130, 165, 185));
      text("SIDE", cx + radius + 8, cy, ofColor(130, 165, 185));
      text("correlation " + ofToString(correlation, 2) + " / vertical = mono", left, edge + height + 15, ofColor(140, 185, 195));
   }
   ofPopMatrix();
   ofPopStyle();
}
