#pragma once
#include <algorithm>
#include <cstddef>
#include <string>

namespace RustelSourceNavigation
{
   // Match the native code panel's UTF-8 character wrapping, including the
   // newline immediately following an exactly full visual row.
   inline int VisualRow(const std::string& text, size_t offset, int columns)
   {
      offset = std::min(offset, text.size());
      size_t begin = 0;
      int row = 0;
      while (begin < text.size())
      {
         size_t end = begin;
         int count = 0;
         while (end < text.size() && text[end] != '\n' && count < std::max(1, columns))
         {
            ++end;
            while (end < text.size() && (static_cast<unsigned char>(text[end]) & 0xc0) == 0x80) ++end;
            ++count;
         }
         if (offset < end || (offset == end && (end == text.size() || text[end] == '\n')))
            return row;
         begin = end;
         if (begin < text.size() && text[begin] == '\n') ++begin;
         ++row;
      }
      return row;
   }
}
