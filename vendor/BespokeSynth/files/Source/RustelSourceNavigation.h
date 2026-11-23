#pragma once
#include <algorithm>
#include <cstddef>
#include <string>

namespace RustelSourceNavigation
{
   inline std::string SingleLine(const std::string& source)
   {
      std::string result;
      size_t begin = 0;
      while (begin < source.size())
      {
         const auto newline = source.find_first_of("\r\n", begin);
         size_t end = newline == std::string::npos ? source.size() : newline;
         size_t first = begin;
         while (first < end && (source[first] == ' ' || source[first] == '\t')) ++first;
         while (end > first && (source[end-1] == ' ' || source[end-1] == '\t')) --end;
         if (end > first)
         {
            if (!result.empty()) result += ' ';
            result.append(source, first, end-first);
         }
         if (newline == std::string::npos) break;
         begin = newline + 1;
      }
      while (!result.empty() && result.back() == ';') result.pop_back();
      return result;
   }

   inline size_t LineStart(const std::string& text, size_t offset)
   {
      offset = std::min(offset, text.size());
      if (offset == 0) return 0;
      const auto newline = text.rfind('\n', offset - 1);
      return newline == std::string::npos ? 0 : newline + 1;
   }

   // Compatibility for producers without parser statement spans: the caller
   // bounds the declaration by the next known binding or playback statement.
   inline size_t TrimTrailingTrivia(const std::string& text, size_t start, size_t end)
   {
      end = std::min(end, text.size());
      while (end > start)
      {
         while (end > start && (text[end-1]==' ' || text[end-1]=='\r' || text[end-1]=='\n' || text[end-1]=='\t')) --end;
         size_t line = LineStart(text, end);
         size_t at = line;
         while (at < end && (text[at]==' ' || text[at]=='\t')) ++at;
         if (at + 1 < end && text[at]=='/' && text[at+1]=='/' && line >= start) end = line;
         else break;
      }
      return end;
   }

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
