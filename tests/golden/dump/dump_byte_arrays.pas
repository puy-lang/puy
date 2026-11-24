program DumpByteArrays;

// The `bytecode` dump of dynamic arrays of bytes (bytecode.md 2.9, 3.2, 5.2): their type entries
// carry `bytes`, and their elements are read and written with `BytesGet` / `BytesSet`, also
// through an open array parameter and in a `for..in`; an array of another one-byte type
// (`ShortInt`) is not one.

type
  TBuf = array of Byte;

function Total(const A: array of Byte): Integer;
var
  B: Byte;
begin
  Result := 0;
  for B in A do
    Result := Result + B;
end;

var
  A: TBuf;
  S: array of ShortInt;
begin
  SetLength(A, 3);
  A[0] := 200;
  A[1] := A[0] div 2;
  S := [-1];
  S[0] := S[0] - 1;
  WriteLn(Total(A), ' ', S[0]);
end.
