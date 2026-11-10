// puy-test: memory-limit=64k
program StringRoomMemoryLimit;

// A string whose length comes from an argument is checked against what is left of the isolate's
// memory limit before its text is built: a length within the length limit (2^31 - 1) but far over
// a 64 KB limit is EOutOfMemory without allocating gigabytes first (semantics.md 3.5).

uses System.SysUtils;

const
  Big = 2147483000;

var
  S: string;
  U: UTF8String;
  N: Integer;
  C: Currency;

procedure Report(const What: string; E: Exception);
begin
  WriteLn(What, ': ', E.ClassName, ': ', E.Message);
end;

// SetLength to the same length allocates nothing, so it works even when the string takes more
// than half of the limit (each in its own routine, so that its string is freed on return)
procedure SameLength;
var
  T: string;
begin
  T := StringOfChar('x', 40000);
  SetLength(T, Length(T));
  WriteLn(Length(T));
end;

procedure SameLengthUtf8;
var
  T: UTF8String;
begin
  SetLength(T, 40000);
  SetLength(T, Length(T));
  WriteLn(Length(T));
end;

begin
  N := Big;
  C := 1.5;
  try S := StringOfChar('x', N); except on E: Exception do Report('StringOfChar', E); end;
  try S := IntToHex(1, N); except on E: Exception do Report('IntToHex', E); end;
  try SetLength(S, N); except on E: Exception do Report('SetLength', E); end;
  try SetLength(U, N); except on E: Exception do Report('SetLength UTF8String', E); end;
  try Write('x':N); except on E: Exception do Report('Write width', E); end;
  try Write(1.5:1:N); except on E: Exception do Report('Write decimals', E); end;
  try S := Format('%*d', [N, 1]); except on E: Exception do Report('Format width', E); end;
  try S := Format('%.*d', [N, 1]); except on E: Exception do Report('Format precision d', E); end;
  try S := Format('%.*f', [N, 1.5]); except on E: Exception do Report('Format precision f', E); end;
  try S := Format('%.*n', [N, C]); except on E: Exception do Report('Format precision n Currency', E); end;
  try S := Format('%.*x', [N, 1]); except on E: Exception do Report('Format precision x', E); end;
  try S := Format('%.*u', [N, 1]); except on E: Exception do Report('Format precision u', E); end;
  try S := Format('%.*e', [N, 1.5]); except on E: Exception do Report('Format precision e', E); end;
  try Write(C:1:N); except on E: Exception do Report('Write decimals Currency', E); end;
  // Lengths that fit in what is left still work
  S := StringOfChar('x', 1000);
  WriteLn(Length(S), ' ', Length(IntToHex(1, 1000)), ' ', Length(Format('%*d', [1000, 1])));
  WriteLn(Length(Format('%.1f', [1.0 / 0.0])));
  S := '';
  SameLength;
  SameLengthUtf8;
end.
