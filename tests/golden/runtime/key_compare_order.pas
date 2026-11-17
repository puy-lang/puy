program KeyCompareOrder;

// The default order of KeyCompare (collections.md 10.2) for each kind of type that has one:
// integers (UInt64 unsigned), Char, Boolean, enumerations, subranges, Currency, strings in code
// point order (as `<`), UTF8String by bytes, the total order of Single / Double in which
// KeyCompare(A, B) = 0 exactly when KeyEquals(A, B), and records and reference counted classes
// with their own LessThan (a descendant uses its nearest ancestor's).

uses System.SysUtils;

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;

  TVersion = record
    Major, Minor: Integer;
    class operator LessThan(const A, B: TVersion): Boolean;
  end;

  TItem = class(TRefObject)
    Rank: Integer;
    constructor Create(ARank: Integer);
    class operator LessThan(const A, B: TItem): Boolean;
  end;

  TSubItem = class(TItem)
  end;

class operator TVersion.LessThan(const A, B: TVersion): Boolean;
begin
  Result := (A.Major < B.Major) or ((A.Major = B.Major) and (A.Minor < B.Minor));
end;

constructor TItem.Create(ARank: Integer);
begin
  Rank := ARank;
end;

class operator TItem.LessThan(const A, B: TItem): Boolean;
begin
  Result := A.Rank < B.Rank;
end;

function Version(Major, Minor: Integer): TVersion;
begin
  Result := Default(TVersion);
  Result.Major := Major;
  Result.Minor := Minor;
end;

var
  Zero: Double;
  D: array[0..6] of Double;
  I, J: Integer;
  Big: UInt64;
  NaN32: Single;
  A, B: TSubItem;
  Line: string;
begin
  WriteLn('Integer: ', KeyCompare(1, 2), ' ', KeyCompare(2, 1), ' ', KeyCompare(3, 3));
  WriteLn('Int64: ', KeyCompare<Int64>(-5, 5));
  Big := High(UInt64);
  WriteLn('UInt64: ', KeyCompare<UInt64>(Big, 1), ' ', KeyCompare<UInt64>(1, Big));
  WriteLn('Char: ', KeyCompare('a', 'b'));
  WriteLn('Boolean: ', KeyCompare(True, False));
  WriteLn('enum: ', KeyCompare(TColor.Red, TColor.Blue), ' ', KeyCompare(TColor.Green, TColor.Green));
  WriteLn('subrange: ', KeyCompare<TDigit>(7, 3));
  WriteLn('Currency: ', KeyCompare<Currency>(1.25, 1.5));
  WriteLn('string: ', KeyCompare('abc', 'abd'), ' ', KeyCompare('b', 'a'), ' ', KeyCompare('ab', 'abc'), ' ', KeyCompare('', ''));
  // U+FF61 comes before U+1F600 in code point order (but not in UTF-16 code unit order)
  WriteLn('string code points: ', KeyCompare<string>(#$FF61, #$D83D#$DE00));
  WriteLn('UTF8String: ', KeyCompare(UTF8String('b'), UTF8String('a')), ' ', KeyCompare(UTF8String(#$FF61), UTF8String(#$D83D#$DE00)));

  // -INF < negative < 0.0 = -0.0 < positive < INF < NaN
  Zero := 0.0;
  D[0] := -1 / Zero;
  D[1] := -2.5;
  D[2] := 0.0;
  D[3] := -Zero;
  D[4] := 3.5;
  D[5] := 1 / Zero;
  D[6] := Zero / Zero;
  WriteLn('Double:');
  for I := 0 to 6 do
  begin
    Line := '';
    for J := 0 to 6 do
      Line := Line + Format('%3d', [KeyCompare(D[I], D[J])]);
    WriteLn(Line);
  end;
  for I := 0 to 6 do
    for J := 0 to 6 do
      if (KeyCompare(D[I], D[J]) = 0) <> KeyEquals(D[I], D[J]) then
        WriteLn('KeyCompare and KeyEquals disagree on ', I, ' and ', J);
  NaN32 := Zero / Zero;
  WriteLn('Single: ', KeyCompare<Single>(NaN32, 1.0), ' ', KeyCompare<Single>(NaN32, NaN32), ' ', KeyCompare<Single>(-Zero, 0.0));

  WriteLn('record: ', KeyCompare(Version(1, 2), Version(1, 3)), ' ', KeyCompare(Version(2, 0), Version(1, 9)), ' ', KeyCompare(Version(1, 2), Version(1, 2)));
  A := TSubItem.Create(5);
  B := TSubItem.Create(3);
  WriteLn('class: ', KeyCompare(A, B), ' ', KeyCompare(B, A), ' ', KeyCompare<TItem>(A, A));
end.
