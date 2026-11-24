unit System.Character;

// The Unicode classification of characters (docs/spec/stdlib/System.Character/README.md):
// TUnicodeCategory and TCharacter. Written from the specification only, not from any other
// implementation. The properties come from the VM's tables through built-ins of the standard
// library: CharClass and CodePointClass give the Unicode properties of a UTF-16 code unit and of
// a code point as a set of bits (1 Alphabetic, 2 Uppercase, 4 Lowercase, 8 White_Space,
// 16 a control character (Cc), 32 a number (Nd, Nl, No), 64 a decimal digit (Nd)),
// UnicodeCategory the general category of a code point as an ordinal of TUnicodeCategory and
// NumericValue its numeric value (-1 if none). The methods taking a Char are those of the Char
// helper of System where it has them.

interface

uses
  System.SysUtils;

type
  // The general categories of Unicode, in the order of their two-letter abbreviations
  // (Cc, Cf, Cn, Co, Cs, Ll, Lm, Lo, Lt, Lu, Mc, Me, Mn, Nd, Nl, No, Pc, Pd, Pe, Pf, Pi, Po, Ps,
  // Sc, Sk, Sm, So, Zl, Zp, Zs)
  TUnicodeCategory = (
    ucControl, ucFormat, ucUnassigned, ucPrivateUse, ucSurrogate,
    ucLowercaseLetter, ucModifierLetter, ucOtherLetter, ucTitlecaseLetter, ucUppercaseLetter,
    ucCombiningMark, ucEnclosingMark, ucNonSpacingMark,
    ucDecimalNumber, ucLetterNumber, ucOtherNumber,
    ucConnectPunctuation, ucDashPunctuation, ucClosePunctuation, ucFinalPunctuation,
    ucInitialPunctuation, ucOtherPunctuation, ucOpenPunctuation,
    ucCurrencySymbol, ucModifierSymbol, ucMathSymbol, ucOtherSymbol,
    ucLineSeparator, ucParagraphSeparator, ucSpaceSeparator);

  // The classification of characters. A Char is one UTF-16 code unit; the methods taking a
  // string and an index (1-based) read the code point there, a surrogate pair as one.
  TCharacter = class sealed
  private
    class function CodePointAt(const S: string; Index: Integer): Integer; static;
    class function ClassAt(const S: string; Index: Integer): Integer; static;
    class function CategoryAt(const S: string; Index: Integer): Integer; static;
  public
    class function ConvertFromUtf32(C: UCS4Char): string; static;
    class function ConvertToUtf32(const S: string; Index: Integer): UCS4Char; overload; static;
    class function ConvertToUtf32(const S: string; Index: Integer;
      out CharLength: Integer): UCS4Char; overload; static;
    class function ConvertToUtf32(const HighSurrogate, LowSurrogate: Char): UCS4Char; overload;
      static;

    class function GetNumericValue(C: Char): Double; overload; static;
    class function GetNumericValue(const S: string; Index: Integer): Double; overload; static;
    class function GetUnicodeCategory(C: Char): TUnicodeCategory; overload; static;
    class function GetUnicodeCategory(const S: string; Index: Integer): TUnicodeCategory;
      overload; static;

    class function IsControl(C: Char): Boolean; overload; static;
    class function IsControl(const S: string; Index: Integer): Boolean; overload; static;
    class function IsDigit(C: Char): Boolean; overload; static;
    class function IsDigit(const S: string; Index: Integer): Boolean; overload; static;
    class function IsHighSurrogate(C: Char): Boolean; overload; static;
    class function IsHighSurrogate(const S: string; Index: Integer): Boolean; overload; static;
    class function IsLetter(C: Char): Boolean; overload; static;
    class function IsLetter(const S: string; Index: Integer): Boolean; overload; static;
    class function IsLetterOrDigit(C: Char): Boolean; overload; static;
    class function IsLetterOrDigit(const S: string; Index: Integer): Boolean; overload; static;
    class function IsLower(C: Char): Boolean; overload; static;
    class function IsLower(const S: string; Index: Integer): Boolean; overload; static;
    class function IsLowSurrogate(C: Char): Boolean; overload; static;
    class function IsLowSurrogate(const S: string; Index: Integer): Boolean; overload; static;
    class function IsNumber(C: Char): Boolean; overload; static;
    class function IsNumber(const S: string; Index: Integer): Boolean; overload; static;
    class function IsPunctuation(C: Char): Boolean; overload; static;
    class function IsPunctuation(const S: string; Index: Integer): Boolean; overload; static;
    class function IsSeparator(C: Char): Boolean; overload; static;
    class function IsSeparator(const S: string; Index: Integer): Boolean; overload; static;
    class function IsSurrogate(C: Char): Boolean; overload; static;
    class function IsSurrogate(const S: string; Index: Integer): Boolean; overload; static;
    class function IsSurrogatePair(const HighSurrogate, LowSurrogate: Char): Boolean; overload;
      static;
    class function IsSurrogatePair(const S: string; Index: Integer): Boolean; overload; static;
    class function IsSymbol(C: Char): Boolean; overload; static;
    class function IsSymbol(const S: string; Index: Integer): Boolean; overload; static;
    class function IsUpper(C: Char): Boolean; overload; static;
    class function IsUpper(const S: string; Index: Integer): Boolean; overload; static;
    class function IsWhiteSpace(C: Char): Boolean; overload; static;
    class function IsWhiteSpace(const S: string; Index: Integer): Boolean; overload; static;

    class function ToLower(C: Char): Char; overload; static;
    class function ToLower(const S: string): string; overload; static;
    class function ToUpper(C: Char): Char; overload; static;
    class function ToUpper(const S: string): string; overload; static;
  end;

implementation

const
  // The ranges of the ordinals of TUnicodeCategory
  FirstPunctuation = Ord(TUnicodeCategory.ucConnectPunctuation);
  LastPunctuation = Ord(TUnicodeCategory.ucOpenPunctuation);
  FirstSymbol = Ord(TUnicodeCategory.ucCurrencySymbol);
  LastSymbol = Ord(TUnicodeCategory.ucOtherSymbol);
  FirstSeparator = Ord(TUnicodeCategory.ucLineSeparator);
  LastSeparator = Ord(TUnicodeCategory.ucSpaceSeparator);

function CodePointText(C: Int64): string;
begin
  Result := 'U+' + IntToHex(C, 4);
end;

// The code point of a surrogate pair (the caller has checked it is one)
function PairCodePoint(High, Low: Char): Integer;
begin
  Result := $10000 + (Ord(High) - $D800) * $400 + (Ord(Low) - $DC00);
end;

{ TCharacter }

class function TCharacter.CodePointAt(const S: string; Index: Integer): Integer;
begin
  if IsSurrogatePair(S, Index) then
    Result := PairCodePoint(S[Index], S[Index + 1])
  else
    Result := Ord(S[Index]);
end;

class function TCharacter.ClassAt(const S: string; Index: Integer): Integer;
begin
  Result := CodePointClass(CodePointAt(S, Index));
end;

class function TCharacter.CategoryAt(const S: string; Index: Integer): Integer;
begin
  Result := UnicodeCategory(CodePointAt(S, Index));
end;

class function TCharacter.ConvertFromUtf32(C: UCS4Char): string;
begin
  if (C > $10FFFF) or ((C >= $D800) and (C <= $DFFF)) then
    raise EArgumentException.Create(CodePointText(C) + ' is not a Unicode scalar value');
  if C < $10000 then
    Result := Char(C)
  else
    Result := Char($D800 + (C - $10000) div $400) + Char($DC00 + (C - $10000) mod $400);
end;

class function TCharacter.ConvertToUtf32(const S: string; Index: Integer): UCS4Char;
var
  C: Char = S[Index];
begin
  if C.IsSurrogate and not IsSurrogatePair(S, Index) then
    raise EArgumentException.Create('the surrogate ' + CodePointText(Ord(C)) + ' at index '
      + IntToStr(Index) + ' does not start a surrogate pair');
  Result := CodePointAt(S, Index);
end;

class function TCharacter.ConvertToUtf32(const S: string; Index: Integer;
  out CharLength: Integer): UCS4Char;
begin
  Result := ConvertToUtf32(S, Index);
  if Result >= $10000 then
    CharLength := 2
  else
    CharLength := 1;
end;

class function TCharacter.ConvertToUtf32(const HighSurrogate, LowSurrogate: Char): UCS4Char;
begin
  if not IsSurrogatePair(HighSurrogate, LowSurrogate) then
    raise EArgumentException.Create('not a surrogate pair: ' + CodePointText(Ord(HighSurrogate))
      + ' ' + CodePointText(Ord(LowSurrogate)));
  Result := PairCodePoint(HighSurrogate, LowSurrogate);
end;

class function TCharacter.GetNumericValue(C: Char): Double;
begin
  Result := NumericValue(Ord(C));
end;

class function TCharacter.GetNumericValue(const S: string; Index: Integer): Double;
begin
  Result := NumericValue(CodePointAt(S, Index));
end;

class function TCharacter.GetUnicodeCategory(C: Char): TUnicodeCategory;
begin
  Result := TUnicodeCategory(UnicodeCategory(Ord(C)));
end;

class function TCharacter.GetUnicodeCategory(const S: string; Index: Integer): TUnicodeCategory;
begin
  Result := TUnicodeCategory(CategoryAt(S, Index));
end;

class function TCharacter.IsControl(C: Char): Boolean;
begin
  Result := C.IsControl;
end;

class function TCharacter.IsControl(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 16) <> 0;
end;

class function TCharacter.IsDigit(C: Char): Boolean;
begin
  Result := C.IsDigit;
end;

class function TCharacter.IsDigit(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 64) <> 0;
end;

class function TCharacter.IsHighSurrogate(C: Char): Boolean;
begin
  Result := C.IsHighSurrogate;
end;

class function TCharacter.IsHighSurrogate(const S: string; Index: Integer): Boolean;
begin
  Result := S[Index].IsHighSurrogate;
end;

class function TCharacter.IsLetter(C: Char): Boolean;
begin
  Result := C.IsLetter;
end;

class function TCharacter.IsLetter(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 1) <> 0;
end;

class function TCharacter.IsLetterOrDigit(C: Char): Boolean;
begin
  Result := C.IsLetterOrDigit;
end;

class function TCharacter.IsLetterOrDigit(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and (1 or 64)) <> 0;
end;

class function TCharacter.IsLower(C: Char): Boolean;
begin
  Result := C.IsLower;
end;

class function TCharacter.IsLower(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 4) <> 0;
end;

class function TCharacter.IsLowSurrogate(C: Char): Boolean;
begin
  Result := C.IsLowSurrogate;
end;

class function TCharacter.IsLowSurrogate(const S: string; Index: Integer): Boolean;
begin
  Result := S[Index].IsLowSurrogate;
end;

class function TCharacter.IsNumber(C: Char): Boolean;
begin
  Result := C.IsNumber;
end;

class function TCharacter.IsNumber(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 32) <> 0;
end;

class function TCharacter.IsPunctuation(C: Char): Boolean;
var
  Cat: Integer = UnicodeCategory(Ord(C));
begin
  Result := (Cat >= FirstPunctuation) and (Cat <= LastPunctuation);
end;

class function TCharacter.IsPunctuation(const S: string; Index: Integer): Boolean;
var
  Cat: Integer = CategoryAt(S, Index);
begin
  Result := (Cat >= FirstPunctuation) and (Cat <= LastPunctuation);
end;

class function TCharacter.IsSeparator(C: Char): Boolean;
var
  Cat: Integer = UnicodeCategory(Ord(C));
begin
  Result := (Cat >= FirstSeparator) and (Cat <= LastSeparator);
end;

class function TCharacter.IsSeparator(const S: string; Index: Integer): Boolean;
var
  Cat: Integer = CategoryAt(S, Index);
begin
  Result := (Cat >= FirstSeparator) and (Cat <= LastSeparator);
end;

class function TCharacter.IsSurrogate(C: Char): Boolean;
begin
  Result := C.IsSurrogate;
end;

class function TCharacter.IsSurrogate(const S: string; Index: Integer): Boolean;
begin
  Result := S[Index].IsSurrogate;
end;

class function TCharacter.IsSurrogatePair(const HighSurrogate, LowSurrogate: Char): Boolean;
begin
  Result := HighSurrogate.IsHighSurrogate and LowSurrogate.IsLowSurrogate;
end;

class function TCharacter.IsSurrogatePair(const S: string; Index: Integer): Boolean;
begin
  Result := S[Index].IsHighSurrogate and (Index < Length(S)) and S[Index + 1].IsLowSurrogate;
end;

class function TCharacter.IsSymbol(C: Char): Boolean;
var
  Cat: Integer = UnicodeCategory(Ord(C));
begin
  Result := (Cat >= FirstSymbol) and (Cat <= LastSymbol);
end;

class function TCharacter.IsSymbol(const S: string; Index: Integer): Boolean;
var
  Cat: Integer = CategoryAt(S, Index);
begin
  Result := (Cat >= FirstSymbol) and (Cat <= LastSymbol);
end;

class function TCharacter.IsUpper(C: Char): Boolean;
begin
  Result := C.IsUpper;
end;

class function TCharacter.IsUpper(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 2) <> 0;
end;

class function TCharacter.IsWhiteSpace(C: Char): Boolean;
begin
  Result := C.IsWhiteSpace;
end;

class function TCharacter.IsWhiteSpace(const S: string; Index: Integer): Boolean;
begin
  Result := (ClassAt(S, Index) and 8) <> 0;
end;

class function TCharacter.ToLower(C: Char): Char;
begin
  Result := C.ToLower;
end;

class function TCharacter.ToLower(const S: string): string;
begin
  Result := AnsiLowerCase(S);
end;

class function TCharacter.ToUpper(C: Char): Char;
begin
  Result := C.ToUpper;
end;

class function TCharacter.ToUpper(const S: string): string;
begin
  Result := AnsiUpperCase(S);
end;

end.
