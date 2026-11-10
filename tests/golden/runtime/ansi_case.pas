program AnsiCase;

// AnsiUpperCase / AnsiLowerCase / AnsiSameText / AnsiCompareText use the Unicode case mapping
// table without language or context rules (semantics.md 5.3, requirements §5.11), unlike the
// ASCII-only UpperCase / SameText / CompareText. By default a character whose mapping is more
// than one character (`ß`, `İ`) stays as it is, and comparisons fold to the single-character
// lowercase; TCaseMapping.cmFull and TCaseFold.cfUpper choose the other rules.

uses System.SysUtils;

const
  Kelvin = #$212A; // KELVIN SIGN, whose lowercase is ASCII `k`

var
  // The option types (and TReplaceFlags) are types of System.SysUtils in declarations too
  M: TCaseMapping;
  F: TCaseFold;
  R: TReplaceFlags;

begin
  // Conversion: Latin, Greek (no final-sigma rule), a surrogate pair, a lone surrogate
  WriteLn(AnsiUpperCase('Ääöü Σσς 𐐨x'), ' ', UpperCase('Ääöü'));
  WriteLn(AnsiLowerCase('ÄÖÜ ΣΑΣ 𐐀X'), ' ', LowerCase('ÄÖÜ'));
  WriteLn(AnsiLowerCase(Kelvin) = 'k', ' ', AnsiUpperCase('k') = 'K');
  WriteLn(Length(AnsiUpperCase('a' + #$D800 + 'b')), ' ', Ord(AnsiUpperCase(#$D800)[1]));
  // Mappings to more than one character: kept by default, expanded with cmFull
  WriteLn(AnsiUpperCase('straße'), ' ', AnsiUpperCase('straße', TCaseMapping.cmSimple), ' ',
    AnsiUpperCase('straße', TCaseMapping.cmFull));
  WriteLn(Length(AnsiLowerCase('İ')), ' ', Length(AnsiLowerCase('İ', TCaseMapping.cmFull)), ' ',
    Ord(AnsiLowerCase('İ', TCaseMapping.cmFull)[2]));
  WriteLn(AnsiLowerCase('ΣΑΣ', TCaseMapping.cmFull));
  // Comparison folded to the lowercase (default) or the uppercase
  WriteLn(AnsiSameText('Ä', 'ä'), ' ', SameText('Ä', 'ä'), ' ', AnsiSameText('𐐨', '𐐀'));
  WriteLn(AnsiSameText('Σ', 'σ'), ' ', AnsiSameText('ς', 'σ'), ' ',
    AnsiSameText('ς', 'σ', TCaseFold.cfUpper));
  WriteLn(AnsiSameText(Kelvin, 'k'), ' ', AnsiSameText(Kelvin, 'K', TCaseFold.cfUpper));
  WriteLn(AnsiSameText('ß', 'SS'), ' ', AnsiSameText('ß', 'ss', TCaseFold.cfUpper));
  WriteLn(AnsiSameText(#$D800, #$D800), ' ', AnsiSameText(#$D800, #$DC00));
  WriteLn(AnsiCompareText('abc', 'ABD'), ' ', AnsiCompareText('ÄB', 'äb'), ' ',
    AnsiCompareText('b', 'A'));
  // Between `Z` and `a`: `_` sorts after the lowercase fold of `A` and before its uppercase fold
  WriteLn(AnsiCompareText('a', '_'), ' ', AnsiCompareText('a', '_', TCaseFold.cfLower), ' ',
    AnsiCompareText('a', '_', TCaseFold.cfUpper), ' ', CompareText('a', '_'));
  // The option as a value of a variable
  M := TCaseMapping.cmFull;
  F := TCaseFold.cfUpper;
  R := [TReplaceFlag.rfReplaceAll];
  WriteLn(AnsiUpperCase('ß', M), ' ', AnsiSameText('ς', 'σ', F), ' ', Ord(M), ' ',
    StringReplace('aa', 'a', 'b', R));
  // Code point order, not code unit order (U+FF21 FULLWIDTH A against a surrogate pair)
  WriteLn(AnsiCompareText(#$FF21, '𐐀'), ' ', AnsiCompareText('𐐨', #$FF41));
end.
