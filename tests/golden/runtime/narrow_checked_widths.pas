program NarrowCheckedWidths;
// Checks every width of NarrowRng (range checks of conversions, {$R+}) and NarrowOvf (overflow
// of narrow arithmetic, {$Q+}) at both ends of the range and one past them: values in range pass
// unchanged, values out of range raise. Covers the fast path of the run loop and the hand-off of
// a failing check to the slow path.
{$R+}{$Q+}
uses System.SysUtils;

var
  N: Int64;
  U: UInt64;

procedure ShowRange(const What: string; V: Int64);
var
  S8: ShortInt;
  U8: Byte;
  S16: SmallInt;
  U16: Word;
  S32: Integer;
  U32: Cardinal;
  C: Char;
begin
  try
    if What = 'ShortInt' then begin S8 := V; WriteLn(What, ' ', S8) end
    else if What = 'Byte' then begin U8 := V; WriteLn(What, ' ', U8) end
    else if What = 'SmallInt' then begin S16 := V; WriteLn(What, ' ', S16) end
    else if What = 'Word' then begin U16 := V; WriteLn(What, ' ', U16) end
    else if What = 'Integer' then begin S32 := V; WriteLn(What, ' ', S32) end
    else if What = 'Cardinal' then begin U32 := V; WriteLn(What, ' ', U32) end
    else if What = 'Char' then begin C := Chr(V); WriteLn(What, ' ', Ord(C)) end;
  except
    on E: Exception do WriteLn(What, ' ', E.ClassName, ': ', E.Message);
  end;
end;

procedure ShowOverflow(A: Integer; B: Integer);
var
  S32: Integer;
begin
  try
    S32 := A + B;
    WriteLn('Integer + ', S32);
  except
    on E: Exception do WriteLn('Integer + ', E.ClassName, ': ', E.Message);
  end;
end;

procedure ShowCardinalOverflow(A: Cardinal; B: Cardinal);
var
  U32: Cardinal;
begin
  try
    U32 := A - B;
    WriteLn('Cardinal - ', U32);
  except
    on E: Exception do WriteLn('Cardinal - ', E.ClassName, ': ', E.Message);
  end;
end;

procedure ShowWide;
var
  I: Int64;
  W: UInt64;
begin
  try
    I := U;
    WriteLn('Int64 ', I);
  except
    on E: Exception do WriteLn('Int64 ', E.ClassName, ': ', E.Message);
  end;
  try
    W := N;
    WriteLn('UInt64 ', W);
  except
    on E: Exception do WriteLn('UInt64 ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  ShowRange('ShortInt', -128);
  ShowRange('ShortInt', 127);
  ShowRange('ShortInt', -129);
  ShowRange('ShortInt', 128);
  ShowRange('Byte', 0);
  ShowRange('Byte', 255);
  ShowRange('Byte', -1);
  ShowRange('Byte', 256);
  ShowRange('SmallInt', -32768);
  ShowRange('SmallInt', 32767);
  ShowRange('SmallInt', -32769);
  ShowRange('SmallInt', 32768);
  ShowRange('Word', 0);
  ShowRange('Word', 65535);
  ShowRange('Word', -1);
  ShowRange('Word', 65536);
  ShowRange('Integer', -2147483648);
  ShowRange('Integer', 2147483647);
  ShowRange('Integer', -2147483649);
  ShowRange('Integer', 2147483648);
  ShowRange('Cardinal', 0);
  ShowRange('Cardinal', 4294967295);
  ShowRange('Cardinal', -1);
  ShowRange('Cardinal', 4294967296);
  ShowRange('Char', 0);
  ShowRange('Char', 65535);
  ShowRange('Char', -1);
  ShowRange('Char', 65536);
  ShowOverflow(2147483646, 1);
  ShowOverflow(2147483647, 1);
  ShowOverflow(-2147483647, -1);
  ShowOverflow(-2147483648, -1);
  ShowCardinalOverflow(1, 1);
  ShowCardinalOverflow(0, 1);
  U := 9223372036854775807;
  N := 0;
  ShowWide;
  U := U + 1;
  N := -1;
  ShowWide;
end.
