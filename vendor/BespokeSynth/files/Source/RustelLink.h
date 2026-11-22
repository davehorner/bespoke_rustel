#pragma once
#include <juce_core/juce_core.h>
#include <string>

// Shared immutable snapshots from the existing background HTTP/Hearsay adapter.
namespace RustelLink
{
   juce::var Sessions();
   juce::var Analysis();
   std::string Status();
   void Solo(const std::string& session, int64_t generation, const std::string& lane, bool enabled);
}
