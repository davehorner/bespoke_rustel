#pragma once

#include <cmath>
#include <string>

namespace RustelNoteGlyph
{
   inline std::string Format(bool pitched, float pitch)
   {
      if (!pitched || !std::isfinite(pitch) || pitch < -256 || pitch > 256)
         return "x";
      static const char* names[] = { "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B" };
      const int midi = int(std::round(pitch));
      const int semitone = (midi % 12 + 12) % 12;
      const int octave = (midi - semitone) / 12 - 1;
      const int cents = int(std::round((pitch - midi) * 100));
      auto text = std::string(names[semitone]) + std::to_string(octave);
      if (cents != 0)
         text += (cents > 0 ? "+" : "") + std::to_string(cents) + "c";
      return text;
   }
}
