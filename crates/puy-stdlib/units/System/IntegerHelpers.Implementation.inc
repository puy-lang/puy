// ===== Integer type helpers (docs/spec/stdlib/System/Helpers.md) =====

// The value of S read as StrToInt64 reads it, checked against the range Lo..Hi of the type
// TypeName: EConvertError when S is not an integer of Int64, ERangeError when the value is out of
// the range.
function ParseIntRange(const S: string; Lo, Hi: Int64; const TypeName: string): Int64;
begin
  Result := StrToInt64(S);
  if (Result < Lo) or (Result > Hi) then
    raise ERangeError.Create('value ' + IntToStr(Result) + ' is out of range for ' + TypeName +
      ' (' + IntToStr(Lo) + '..' + IntToStr(Hi) + ')');
end;

// Whether S reads as an integer of Int64 in the range Lo..Hi (ParseIntRange without raising).
function TryParseIntRange(const S: string; Lo, Hi: Int64; out Value: Int64): Boolean;
begin
  Value := 0;
  try
    Value := StrToInt64(S);
  except
    on EConvertError do
      Exit(False);
  end;
  Result := (Value >= Lo) and (Value <= Hi);
  if not Result then
    Value := 0;
end;

// Whether S reads as a UInt64: the form of StrToInt64 (leading spaces and tabs, an optional +,
// decimal digits or $ and hexadecimal digits, nothing after them), without a minus sign. Value
// is 0 when it does not.
function TryParseUInt64(const S: string; out Value: UInt64): Boolean;
var
  I, N, D: Integer;
  Hex: Boolean;
  C: Char;
  V: UInt64;
begin
  Value := 0;
  Result := False;
  N := Length(S);
  I := 1;
  while (I <= N) and ((S[I] = ' ') or (S[I] = #9)) do
    Inc(I);
  if (I <= N) and (S[I] = '+') then
    Inc(I);
  Hex := (I <= N) and (S[I] = '$');
  if Hex then
    Inc(I);
  if I > N then
    Exit;
  V := 0;
  while I <= N do
  begin
    C := S[I];
    if (C >= '0') and (C <= '9') then
      D := Ord(C) - Ord('0')
    else if Hex and (C >= 'A') and (C <= 'F') then
      D := Ord(C) - Ord('A') + 10
    else if Hex and (C >= 'a') and (C <= 'f') then
      D := Ord(C) - Ord('a') + 10
    else
      Exit;
    if Hex then
    begin
      if V shr 60 <> 0 then
        Exit;
      V := V shl 4 + UInt64(D);
    end
    else
    begin
      if V > (High(UInt64) - UInt64(D)) div 10 then
        Exit;
      V := V * 10 + UInt64(D);
    end;
    Inc(I);
  end;
  Value := V;
  Result := True;
end;

// The Count lowest bytes of V, the lowest first.
function LittleEndianBytes(V: UInt64; Count: Integer): TArray<Byte>;
var
  I: Integer;
begin
  SetLength(Result, Count);
  for I := 0 to Count - 1 do
    Result[I] := Byte((V shr (8 * I)) and $FF);
end;

// The Count lowest bytes of the two's complement of V, the lowest first.
function SignedLittleEndianBytes(V: Int64; Count: Integer): TArray<Byte>;
var
  I: Integer;
begin
  SetLength(Result, Count);
  for I := 0 to Count - 1 do
    Result[I] := Byte((V shr (8 * I)) and $FF);
end;

// --- TByteHelper ---

class function TByteHelper.Parse(const S: string): Byte;
begin
  Result := ParseIntRange(S, Low(Byte), High(Byte), 'Byte');
end;

class function TByteHelper.TryParse(const S: string; out Value: Byte): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(Byte), High(Byte), V);
  Value := V;
end;

function TByteHelper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TByteHelper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(Byte));
end;

function TByteHelper.ToDouble: Double;
begin
  Result := Self;
end;

function TByteHelper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(Byte));
end;

function TByteHelper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TByteHelper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TShortIntHelper ---

class function TShortIntHelper.Parse(const S: string): ShortInt;
begin
  Result := ParseIntRange(S, Low(ShortInt), High(ShortInt), 'ShortInt');
end;

class function TShortIntHelper.TryParse(const S: string; out Value: ShortInt): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(ShortInt), High(ShortInt), V);
  Value := V;
end;

function TShortIntHelper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TShortIntHelper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(ShortInt));
end;

function TShortIntHelper.ToDouble: Double;
begin
  Result := Self;
end;

function TShortIntHelper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(ShortInt));
end;

function TShortIntHelper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TShortIntHelper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TWordHelper ---

class function TWordHelper.Parse(const S: string): Word;
begin
  Result := ParseIntRange(S, Low(Word), High(Word), 'Word');
end;

class function TWordHelper.TryParse(const S: string; out Value: Word): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(Word), High(Word), V);
  Value := V;
end;

function TWordHelper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TWordHelper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(Word));
end;

function TWordHelper.ToDouble: Double;
begin
  Result := Self;
end;

function TWordHelper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(Word));
end;

function TWordHelper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TWordHelper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TSmallIntHelper ---

class function TSmallIntHelper.Parse(const S: string): SmallInt;
begin
  Result := ParseIntRange(S, Low(SmallInt), High(SmallInt), 'SmallInt');
end;

class function TSmallIntHelper.TryParse(const S: string; out Value: SmallInt): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(SmallInt), High(SmallInt), V);
  Value := V;
end;

function TSmallIntHelper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TSmallIntHelper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(SmallInt));
end;

function TSmallIntHelper.ToDouble: Double;
begin
  Result := Self;
end;

function TSmallIntHelper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(SmallInt));
end;

function TSmallIntHelper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TSmallIntHelper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TCardinalHelper ---

class function TCardinalHelper.Parse(const S: string): Cardinal;
begin
  Result := ParseIntRange(S, Low(Cardinal), High(Cardinal), 'Cardinal');
end;

class function TCardinalHelper.TryParse(const S: string; out Value: Cardinal): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(Cardinal), High(Cardinal), V);
  Value := V;
end;

function TCardinalHelper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TCardinalHelper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(Cardinal));
end;

function TCardinalHelper.ToDouble: Double;
begin
  Result := Self;
end;

function TCardinalHelper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(Cardinal));
end;

function TCardinalHelper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TCardinalHelper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TIntegerHelper ---

class function TIntegerHelper.Parse(const S: string): Integer;
begin
  Result := ParseIntRange(S, Low(Integer), High(Integer), 'Integer');
end;

class function TIntegerHelper.TryParse(const S: string; out Value: Integer): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(Integer), High(Integer), V);
  Value := V;
end;

function TIntegerHelper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TIntegerHelper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(Integer));
end;

function TIntegerHelper.ToDouble: Double;
begin
  Result := Self;
end;

function TIntegerHelper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(Integer));
end;

function TIntegerHelper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TIntegerHelper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TUInt64Helper ---

class function TUInt64Helper.Parse(const S: string): UInt64;
var
  V: Int64;
begin
  if TryParseUInt64(S, Result) then
    Exit;
  // Not a UInt64: StrToInt64 raises the EConvertError, or the value is negative
  V := StrToInt64(S);
  if V < 0 then
    raise ERangeError.Create('value ' + IntToStr(V) + ' is out of range for UInt64 (0..' +
      IntToStr(High(UInt64)) + ')');
  Result := V;
end;

class function TUInt64Helper.TryParse(const S: string; out Value: UInt64): Boolean;
var
  V: Int64;
begin
  Result := TryParseUInt64(S, Value);
  // A minus sign before a zero, as Parse reads it
  if not Result and TryParseIntRange(S, 0, 0, V) then
  begin
    Value := V;
    Result := True;
  end;
end;

function TUInt64Helper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TUInt64Helper.ToBytes: TArray<Byte>;
begin
  Result := LittleEndianBytes(Self, SizeOf(UInt64));
end;

function TUInt64Helper.ToDouble: Double;
begin
  Result := Self;
end;

function TUInt64Helper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(UInt64));
end;

function TUInt64Helper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TUInt64Helper.ToSingle: Single;
begin
  Result := Self;
end;

// --- TInt64Helper ---

class function TInt64Helper.Parse(const S: string): Int64;
begin
  Result := ParseIntRange(S, Low(Int64), High(Int64), 'Int64');
end;

class function TInt64Helper.TryParse(const S: string; out Value: Int64): Boolean;
var
  V: Int64;
begin
  Result := TryParseIntRange(S, Low(Int64), High(Int64), V);
  Value := V;
end;

function TInt64Helper.ToBoolean: Boolean;
begin
  Result := Self <> 0;
end;

function TInt64Helper.ToBytes: TArray<Byte>;
begin
  Result := SignedLittleEndianBytes(Self, SizeOf(Int64));
end;

function TInt64Helper.ToDouble: Double;
begin
  Result := Self;
end;

function TInt64Helper.ToHexString: string;
begin
  Result := IntToHex(Self, 2 * SizeOf(Int64));
end;

function TInt64Helper.ToHexString(MinDigits: Integer): string;
begin
  Result := IntToHex(Self, MinDigits);
end;

function TInt64Helper.ToSingle: Single;
begin
  Result := Self;
end;
