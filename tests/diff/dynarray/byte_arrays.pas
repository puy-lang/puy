program ByteArrays;

// Dynamic arrays of `Byte`, which the VM holds one byte per element (types.md 9; requirements
// §5.5), behave as the other dynamic arrays: element reads and writes, SetLength growing and
// shrinking, sharing, Copy, `for..in`, an element passed to a `var` parameter, open array
// parameters given dynamic and static arrays, a value open array parameter written by the
// callee, and arrays of arrays of bytes. (`+`, Concat, Insert and Delete, which Delphi XE4 lacks,
// slices, TList<Byte> and the other generic uses are in golden/runtime/byte_arrays_puy.)

type
  TByteArr = array of Byte;

procedure Show(const Title: string; const A: array of Byte);
var
  I: Integer;
begin
  Write(Title, ' (', Length(A), '):');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn;
end;

function Sum(const A: array of Byte): Integer;
var
  B: Byte;
begin
  Result := 0;
  for B in A do
    Result := Result + B;
end;

procedure Bump(var B: Byte);
begin
  B := B + 10;
end;

procedure Scramble(A: array of Byte);
begin
  A[0] := 255;
  Show('scrambled', A);
end;

var
  A, B, C: TByteArr;
  S: array[0..2] of Byte;
  G: array of TByteArr;
  I: Integer;
  X: Byte;
begin
  SetLength(A, 5);
  Show('new', A);
  for I := 0 to 4 do
    A[I] := I * 60;
  Show('filled', A);
  X := A[4];
  WriteLn('A[4] = ', X, ', A[1] + A[4] = ', A[1] + A[4]);
  B := A;
  B[0] := 7;
  Show('shared', A);
  SetLength(A, 7);
  A[6] := 200;
  Show('grown', A);
  Show('other', B);
  SetLength(A, 2);
  Show('shrunk', A);
  SetLength(A, 4);
  Show('regrown', A);
  C := Copy(B, 1, 3);
  C[0] := 1;
  Show('copy', C);
  Show('source', B);
  WriteLn('sum ', Sum(B));
  Write('for..in:');
  for X in B do
    Write(' ', X);
  WriteLn;
  Bump(B[2]);
  Show('bumped', B);
  Inc(B[3], 3);
  Dec(B[0]);
  Show('inc dec', B);
  S[0] := 1;
  S[1] := 2;
  S[2] := 3;
  Show('static', S);
  WriteLn('static sum ', Sum(S));
  Scramble(B);
  Show('kept', B);
  Scramble(S);
  Show('kept static', S);
  SetLength(G, 2, 3);
  G[1][2] := 42;
  G[0][0] := G[1][2] + 1;
  Show('G[0]', G[0]);
  Show('G[1]', G[1]);
  A := nil;
  Show('nil', A);
  WriteLn('empty sum ', Sum(A));
end.
