// ===== Edit (docs/spec/stdlib/System.StrUtils/README.md 4) =====

function ReplaceStr(const AText, AFromText, AToText: string): string;
begin
  Result := AText.Replace(AFromText, AToText);
end;

function ReplaceText(const AText, AFromText, AToText: string): string;
begin
  Result := StringReplace(AText, AFromText, AToText,
    [TReplaceFlag.rfReplaceAll, TReplaceFlag.rfIgnoreCase]);
end;

function AnsiReplaceStr(const AText, AFromText, AToText: string): string;
begin
  Result := ReplaceStr(AText, AFromText, AToText);
end;

function AnsiReplaceText(const AText, AFromText, AToText: string): string;
begin
  Result := ReplaceText(AText, AFromText, AToText);
end;

function LeftStr(const AText: string; const ACount: Integer): string;
begin
  Result := AText.Substring(0, ACount);
end;

function RightStr(const AText: string; const ACount: Integer): string;
begin
  if ACount <= 0 then
    Exit('');
  // A negative start (ACount above the length) is cut to 0: the whole string
  Result := AText.Substring(Length(AText) - ACount);
end;

function MidStr(const AText: string; const AStart, ACount: Integer): string;
begin
  Result := Copy(AText, AStart, ACount);
end;

function AnsiLeftStr(const AText: string; const ACount: Integer): string;
begin
  Result := LeftStr(AText, ACount);
end;

function AnsiRightStr(const AText: string; const ACount: Integer): string;
begin
  Result := RightStr(AText, ACount);
end;

function AnsiMidStr(const AText: string; const AStart, ACount: Integer): string;
begin
  Result := MidStr(AText, AStart, ACount);
end;

function ReverseString(const AText: string): string;
var
  Len, I, J: Integer;
begin
  Len := Length(AText);
  Result := '';
  SetLength(Result, Len);
  I := Len;
  J := 1;
  while I >= 1 do
  begin
    // A low surrogate after a high one: the pair goes over in its own order
    if (I > 1) and (AText[I] >= #$DC00) and (AText[I] <= #$DFFF) and (AText[I - 1] >= #$D800) and
      (AText[I - 1] <= #$DBFF) then
    begin
      Result[J] := AText[I - 1];
      Result[J + 1] := AText[I];
      Inc(J, 2);
      Dec(I, 2);
    end
    else
    begin
      Result[J] := AText[I];
      Inc(J);
      Dec(I);
    end;
  end;
end;

function AnsiReverseString(const AText: string): string;
begin
  Result := ReverseString(AText);
end;

// Doubles the piece for each bit of ACount, so that a long result takes few concatenations. The
// piece is doubled only while a higher bit is left, so it never grows past the result.
function DupeString(const AText: string; ACount: Integer): string;
var
  Piece: string;
begin
  Result := '';
  if (ACount <= 0) or (AText = '') then
    Exit;
  // The length is checked before anything is built, as the operations of the VM do
  // (docs/spec/semantics.md 3.5), so that a result too long takes no memory
  if Int64(Length(AText)) * ACount > MaxInt then
    raise EOutOfMemory.Create('out of memory');
  Piece := AText;
  while ACount > 0 do
  begin
    if Odd(ACount) then
      Result := Result + Piece;
    ACount := ACount shr 1;
    if ACount > 0 then
      Piece := Piece + Piece;
  end;
end;

function StuffString(const AText: string; AStart, ALength: Integer; const ASubText: string): string;
var
  Head, Tail: Int64;
begin
  if ALength < 0 then
    ALength := 0;
  // The characters before AStart, and those from AStart + ALength on; computed in Int64 so that
  // no sum overflows, and cut to the string as Copy does
  Head := Int64(AStart) - 1;
  if Head < 0 then
    Head := 0;
  if Head > Length(AText) then
    Head := Length(AText);
  Tail := Int64(AStart) + ALength;
  if Tail < 1 then
    Tail := 1;
  if Tail > Int64(Length(AText)) + 1 then
    Tail := Int64(Length(AText)) + 1;
  Result := Copy(AText, 1, Integer(Head)) + ASubText + Copy(AText, Integer(Tail), MaxInt);
end;

function SplitString(const S, Delimiters: string): TStringDynArray;
var
  Separators: TArray<string>;
  I: Integer;
begin
  Separators := nil;
  SetLength(Separators, Length(Delimiters));
  for I := 1 to Length(Delimiters) do
    Separators[I - 1] := Delimiters[I];
  Result := S.Split(Separators);
end;
