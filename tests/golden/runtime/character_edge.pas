// System.Character beyond what the diff case checks against FPC: the numeric value -1 of a
// character without one, lone surrogates in the methods taking a string and an index, the
// properties of the supplementary planes, the Unicode properties that differ from the general
// category, the ordinals of TUnicodeCategory (Delphi's; FPC orders them differently), and the
// exceptions (docs/spec/stdlib/System.Character/README.md).
program CharacterEdge;

uses
  System.SysUtils, System.Character;

procedure Check(const Name: string; const P: TProc);
begin
  try
    P();
    WriteLn(Name, ': no exception');
  except
    on E: Exception do
      WriteLn(Name, ': ', E.ClassName, ': ', E.Message);
  end;
end;

var
  S, Lone: string;
  Len: Integer;
  Cat: TUnicodeCategory;
begin
  // The ordinals of TUnicodeCategory: the order of the two-letter abbreviations
  WriteLn(Ord(TUnicodeCategory.ucControl), ' ', Ord(TUnicodeCategory.ucSurrogate), ' ',
    Ord(TUnicodeCategory.ucUppercaseLetter), ' ', Ord(TUnicodeCategory.ucDecimalNumber), ' ',
    Ord(TUnicodeCategory.ucOpenPunctuation), ' ', Ord(TUnicodeCategory.ucSpaceSeparator));
  for Cat := Low(TUnicodeCategory) to High(TUnicodeCategory) do
    Write(Ord(Cat), ' ');
  WriteLn;
  WriteLn(Ord(TCharacter.GetUnicodeCategory(#$0007)), ' ', Ord(TCharacter.GetUnicodeCategory('A')),
    ' ', Ord(TCharacter.GetUnicodeCategory(' ')));

  // A character without a numeric value is -1; field 8 of UnicodeData.txt only
  WriteLn(TCharacter.GetNumericValue('a'):0:1, ' ', TCharacter.GetNumericValue(#$5146):0:1, ' ',
    TCharacter.GetNumericValue(#$0F33):0:2, ' ', TCharacter.GetNumericValue(#$D800):0:1);
  S := TCharacter.ConvertFromUtf32($16B61) + TCharacter.ConvertFromUtf32($1D7D7);
  WriteLn(TCharacter.GetNumericValue(S, 1):0:0, ' ', TCharacter.GetNumericValue(S, 3):0:0, ' ',
    TCharacter.GetNumericValue(S, 2):0:0);

  // The Unicode properties, not only the general category: a Roman numeral is Alphabetic and
  // Uppercase, U+0345 (a mark) is Alphabetic and Lowercase, U+200B is no White_Space
  WriteLn(TCharacter.IsLetter(#$2163), ' ', TCharacter.IsUpper(#$2163), ' ',
    Ord(TCharacter.GetUnicodeCategory(#$2163)));
  WriteLn(TCharacter.IsLetter(#$0345), ' ', TCharacter.IsLower(#$0345), ' ',
    Ord(TCharacter.GetUnicodeCategory(#$0345)));
  WriteLn(TCharacter.IsWhiteSpace(#$200B), ' ', Ord(TCharacter.GetUnicodeCategory(#$200B)), ' ',
    TCharacter.IsWhiteSpace(#$0085), ' ', TCharacter.IsSeparator(#$0085));

  // The supplementary planes: a letter, an emoji, a digit, a private use and an unassigned code
  // point
  S := TCharacter.ConvertFromUtf32($20000) + TCharacter.ConvertFromUtf32($1F600)
    + TCharacter.ConvertFromUtf32($1D7CE) + TCharacter.ConvertFromUtf32($F0000)
    + TCharacter.ConvertFromUtf32($10FFFF);
  WriteLn(Ord(TCharacter.GetUnicodeCategory(S, 1)), ' ', Ord(TCharacter.GetUnicodeCategory(S, 3)),
    ' ', Ord(TCharacter.GetUnicodeCategory(S, 5)), ' ', Ord(TCharacter.GetUnicodeCategory(S, 7)),
    ' ', Ord(TCharacter.GetUnicodeCategory(S, 9)));
  WriteLn(TCharacter.IsLetter(S, 1), ' ', TCharacter.IsSymbol(S, 3), ' ', TCharacter.IsDigit(S, 5),
    ' ', TCharacter.IsNumber(S, 5));

  // A lone surrogate, and the second half of a pair, are the code unit: category Cs and no
  // other property
  Lone := 'a' + #$D800 + 'b' + #$DC00;
  WriteLn(Ord(TCharacter.GetUnicodeCategory(Lone, 2)), ' ', TCharacter.IsLetter(Lone, 2), ' ',
    TCharacter.IsSurrogatePair(Lone, 2), ' ', TCharacter.IsSurrogatePair(Lone, 4));
  WriteLn(Ord(TCharacter.GetUnicodeCategory(S, 2)), ' ', TCharacter.IsLetter(S, 2), ' ',
    TCharacter.IsLowSurrogate(S, 2));
  WriteLn(TCharacter.ConvertToUtf32('x' + TCharacter.ConvertFromUtf32($1F600), 2, Len), ' ', Len);

  // The case conversions of a string keep a character that maps to more than one
  WriteLn(TCharacter.ToUpper('straße'), ' ', Ord(TCharacter.ToLower(#$0130)), ' ',
    Length(TCharacter.ToLower(string(#$0130))));

  Check('ConvertFromUtf32($110000)', procedure begin TCharacter.ConvertFromUtf32($110000); end);
  Check('ConvertFromUtf32($D800)', procedure begin TCharacter.ConvertFromUtf32($D800); end);
  Check('ConvertFromUtf32($DFFF)', procedure begin TCharacter.ConvertFromUtf32($DFFF); end);
  Check('ConvertToUtf32(high, a)', procedure begin TCharacter.ConvertToUtf32(#$D800, 'a'); end);
  Check('ConvertToUtf32(low, high)',
    procedure begin TCharacter.ConvertToUtf32(#$DC00, #$D800); end);
  Check('ConvertToUtf32(lone high)', procedure begin TCharacter.ConvertToUtf32(Lone, 2); end);
  Check('ConvertToUtf32(lone low)', procedure begin TCharacter.ConvertToUtf32(Lone, 4); end);
  Check('ConvertToUtf32(second half)', procedure begin TCharacter.ConvertToUtf32(S, 2); end);
  Check('ConvertToUtf32(high at the end)',
    procedure begin TCharacter.ConvertToUtf32('a' + #$D83D, 2, Len); end);
  Check('IsLetter(S, 0)', procedure begin WriteLn(TCharacter.IsLetter('abc', 0)); end);
  Check('IsLetter(S, 4)', procedure begin WriteLn(TCharacter.IsLetter('abc', 4)); end);
  Check('IsSurrogatePair('''', 1)', procedure begin WriteLn(TCharacter.IsSurrogatePair('', 1)); end);
end.
