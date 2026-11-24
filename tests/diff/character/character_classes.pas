// TCharacter of System.Character: the general categories (by name; the ordinals of
// TUnicodeCategory are in golden/runtime/character_edge) and the classification methods on
// characters whose properties have been stable across Unicode versions, the methods taking a
// string and an index on surrogate pairs, the UTF-32 conversions and the numeric values.
program CharacterClasses;

uses
  System.SysUtils, System.Character;

// The abbreviation of a general category, written with a case so that the output does not
// depend on the ordinals of TUnicodeCategory
function Abbrev(Cat: TUnicodeCategory): string;
begin
  case Cat of
    TUnicodeCategory.ucControl: Result := 'Cc';
    TUnicodeCategory.ucFormat: Result := 'Cf';
    TUnicodeCategory.ucUnassigned: Result := 'Cn';
    TUnicodeCategory.ucPrivateUse: Result := 'Co';
    TUnicodeCategory.ucSurrogate: Result := 'Cs';
    TUnicodeCategory.ucLowercaseLetter: Result := 'Ll';
    TUnicodeCategory.ucModifierLetter: Result := 'Lm';
    TUnicodeCategory.ucOtherLetter: Result := 'Lo';
    TUnicodeCategory.ucTitlecaseLetter: Result := 'Lt';
    TUnicodeCategory.ucUppercaseLetter: Result := 'Lu';
    TUnicodeCategory.ucCombiningMark: Result := 'Mc';
    TUnicodeCategory.ucEnclosingMark: Result := 'Me';
    TUnicodeCategory.ucNonSpacingMark: Result := 'Mn';
    TUnicodeCategory.ucDecimalNumber: Result := 'Nd';
    TUnicodeCategory.ucLetterNumber: Result := 'Nl';
    TUnicodeCategory.ucOtherNumber: Result := 'No';
    TUnicodeCategory.ucConnectPunctuation: Result := 'Pc';
    TUnicodeCategory.ucDashPunctuation: Result := 'Pd';
    TUnicodeCategory.ucClosePunctuation: Result := 'Pe';
    TUnicodeCategory.ucFinalPunctuation: Result := 'Pf';
    TUnicodeCategory.ucInitialPunctuation: Result := 'Pi';
    TUnicodeCategory.ucOtherPunctuation: Result := 'Po';
    TUnicodeCategory.ucOpenPunctuation: Result := 'Ps';
    TUnicodeCategory.ucCurrencySymbol: Result := 'Sc';
    TUnicodeCategory.ucModifierSymbol: Result := 'Sk';
    TUnicodeCategory.ucMathSymbol: Result := 'Sm';
    TUnicodeCategory.ucOtherSymbol: Result := 'So';
    TUnicodeCategory.ucLineSeparator: Result := 'Zl';
    TUnicodeCategory.ucParagraphSeparator: Result := 'Zp';
    TUnicodeCategory.ucSpaceSeparator: Result := 'Zs';
  else
    Result := '?';
  end;
end;

procedure ShowCategory(const Name: string; C: Char);
begin
  WriteLn(Name, ' ', Abbrev(TCharacter.GetUnicodeCategory(C)));
end;

procedure ShowClasses(const Name: string; C: Char);
begin
  WriteLn(Name, ' letter=', TCharacter.IsLetter(C), ' digit=', TCharacter.IsDigit(C),
    ' number=', TCharacter.IsNumber(C), ' upper=', TCharacter.IsUpper(C),
    ' lower=', TCharacter.IsLower(C), ' space=', TCharacter.IsWhiteSpace(C),
    ' control=', TCharacter.IsControl(C), ' punct=', TCharacter.IsPunctuation(C),
    ' symbol=', TCharacter.IsSymbol(C), ' sep=', TCharacter.IsSeparator(C),
    ' letdig=', TCharacter.IsLetterOrDigit(C));
end;

var
  S, T: string;
  Len: Integer;
begin
  // One character of each category
  ShowCategory('Cc', #$0007);
  ShowCategory('Cf', #$200E);
  ShowCategory('Cn', #$0378);
  ShowCategory('Co', #$E000);
  ShowCategory('Cs', #$D800);
  ShowCategory('Ll', 'a');
  ShowCategory('Lm', #$02B0);
  ShowCategory('Lo', #$3042);
  ShowCategory('Lt', #$01C5);
  ShowCategory('Lu', 'A');
  ShowCategory('Mc', #$0903);
  ShowCategory('Me', #$20DD);
  ShowCategory('Mn', #$0301);
  ShowCategory('Nd', '7');
  ShowCategory('Nl', #$2163);
  ShowCategory('No', #$00BD);
  ShowCategory('Pc', '_');
  ShowCategory('Pd', '-');
  ShowCategory('Pe', ')');
  ShowCategory('Pf', #$00BB);
  ShowCategory('Pi', #$00AB);
  ShowCategory('Po', '!');
  ShowCategory('Ps', '(');
  ShowCategory('Sc', '$');
  ShowCategory('Sk', '^');
  ShowCategory('Sm', '+');
  ShowCategory('So', #$00A9);
  ShowCategory('Zl', #$2028);
  ShowCategory('Zp', #$2029);
  ShowCategory('Zs', ' ');

  // The classification methods
  ShowClasses('A', 'A');
  ShowClasses('z', 'z');
  ShowClasses('5', '5');
  ShowClasses('fullwidth 7', #$FF17);
  ShowClasses('half', #$00BD);
  ShowClasses('space', ' ');
  ShowClasses('tab', #9);
  ShowClasses('nbsp', #$00A0);
  ShowClasses('ideographic space', #$3000);
  ShowClasses('line sep', #$2028);
  ShowClasses('comma', ',');
  ShowClasses('plus', '+');
  ShowClasses('yen', #$00A5);
  ShowClasses('hiragana a', #$3042);
  ShowClasses('high surrogate', #$D800);

  // The methods taking a string and an index (1-based): a surrogate pair is one code point
  S := 'a' + TCharacter.ConvertFromUtf32($10400) + '1' + TCharacter.ConvertFromUtf32($1D11E);
  WriteLn(Length(S));
  WriteLn(TCharacter.IsLetter(S, 1), ' ', TCharacter.IsUpper(S, 2), ' ', TCharacter.IsDigit(S, 4),
    ' ', TCharacter.IsSymbol(S, 5));
  WriteLn(Abbrev(TCharacter.GetUnicodeCategory(S, 2)), ' ',
    Abbrev(TCharacter.GetUnicodeCategory(S, 5)));
  WriteLn(TCharacter.IsSurrogate(S, 2), ' ', TCharacter.IsHighSurrogate(S, 2), ' ',
    TCharacter.IsLowSurrogate(S, 3), ' ', TCharacter.IsSurrogatePair(S, 2), ' ',
    TCharacter.IsSurrogatePair(S, 3), ' ', TCharacter.IsSurrogatePair(S, 1));
  WriteLn(TCharacter.IsSurrogatePair(#$D83D, #$DE00), ' ', TCharacter.IsSurrogatePair(#$DE00, #$D83D));

  // The UTF-32 conversions
  WriteLn(TCharacter.ConvertToUtf32(S, 1), ' ', TCharacter.ConvertToUtf32(S, 2), ' ',
    TCharacter.ConvertToUtf32(S, 5));
  WriteLn(TCharacter.ConvertToUtf32(S, 2, Len), ' ', Len);
  WriteLn(TCharacter.ConvertToUtf32(S, 4, Len), ' ', Len);
  WriteLn(TCharacter.ConvertToUtf32(#$D83D, #$DE00));
  T := TCharacter.ConvertFromUtf32($1F600);
  WriteLn(Length(T), ' ', Ord(T[1]), ' ', Ord(T[2]));
  T := TCharacter.ConvertFromUtf32($41);
  WriteLn(Length(T), ' ', T);
  T := TCharacter.ConvertFromUtf32($FFFF);
  WriteLn(Length(T), ' ', Ord(T[1]));
  T := TCharacter.ConvertFromUtf32($10FFFF);
  WriteLn(Length(T), ' ', Ord(T[1]), ' ', Ord(T[2]));

  // The numeric values
  WriteLn(TCharacter.GetNumericValue('0'):0:4, ' ', TCharacter.GetNumericValue('9'):0:4, ' ',
    TCharacter.GetNumericValue(#$00BD):0:4, ' ',
    TCharacter.GetNumericValue(#$00BE):0:4, ' ', TCharacter.GetNumericValue(#$2163):0:4, ' ',
    TCharacter.GetNumericValue(#$FF17):0:4, ' ', TCharacter.GetNumericValue(#$0663):0:4);
  WriteLn(TCharacter.GetNumericValue(S, 4):0:4);

  // The case conversions
  WriteLn(TCharacter.ToUpper('a'), TCharacter.ToLower('B'), ' ', TCharacter.ToUpper('abc'), ' ',
    TCharacter.ToLower('ÀÉÎ'), ' ', TCharacter.ToUpper('àéî'));
end.
