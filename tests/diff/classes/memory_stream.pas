program MemoryStreamBasics;

// The streams in memory of System.Classes (docs/spec/stdlib/System.Classes/TStream.md) as
// Delphi's: writing and reading TBytes, Position / Size / Seek, a short read at the end,
// seeking past the end, setting the size, CopyFrom, TBytesStream on an array and TStringStream.
// Streams are reference counted in Puy, which does not allow Free; the other compilers keep
// them until the program ends. Only the TBytes forms with an offset: the others are untyped
// parameters in Delphi.

uses System.SysUtils, System.Classes;

procedure ShowBytes(const B: TBytes; N: Integer);
var
  I: Integer;
begin
  for I := 0 to N - 1 do
    Write(B[I], ' ');
  WriteLn;
end;

var
  M, C: TMemoryStream;
  B, R: TBytes;
  I: Integer;
  S: TBytesStream;
  T: TStringStream;
begin
  M := TMemoryStream.Create;
  SetLength(B, 600);
  for I := 0 to 599 do
    B[I] := I mod 256;
  WriteLn(M.Write(B, 0, 600));
  WriteLn(M.Size, ' ', M.Position);
  WriteLn(M.Seek(250, TSeekOrigin.soBeginning), ' ', M.Seek(10, TSeekOrigin.soCurrent), ' ',
    M.Seek(-5, TSeekOrigin.soEnd));
  SetLength(R, 20);
  WriteLn(M.Read(R, 0, 20));
  ShowBytes(R, 5);
  WriteLn(M.Read(R, 0, 20), ' ', M.Position);
  M.Position := 254;
  WriteLn(M.Read(R, 2, 4));
  ShowBytes(R, 6);
  M.Position := 610;
  WriteLn(M.Write(B, 3, 1));
  WriteLn(M.Size, ' ', M.Position);
  M.Size := 300;
  WriteLn(M.Size, ' ', M.Position);
  C := TMemoryStream.Create;
  M.Position := 290;
  WriteLn(C.CopyFrom(M, 10), ' ', C.Size, ' ', M.Position);
  WriteLn(C.CopyFrom(M, 0), ' ', C.Size);
  C.Position := 0;
  WriteLn(C.Read(R, 0, 3));
  ShowBytes(R, 3);

  SetLength(B, 3);
  B[0] := 7;
  B[1] := 8;
  B[2] := 9;
  S := TBytesStream.Create(B);
  WriteLn(S.Size, ' ', S.Position);
  WriteLn(S.Read(R, 0, 5));
  ShowBytes(R, 3);

  T := TStringStream.Create('abc');
  WriteLn(T.Size, ' ', T.DataString, ' ', T.ReadString(2), ' ', T.Position);
  T.Position := T.Size;
  T.WriteString('de');
  WriteLn(T.DataString, ' ', T.Size);
end.
