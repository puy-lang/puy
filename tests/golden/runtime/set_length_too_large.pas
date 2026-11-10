program SetLengthTooLarge;

// A length above 2^31 - 1 is EOutOfMemory, decided before anything is allocated
// (types.md 2.3).

var
  A: array of Byte;
  N: Int64;
begin
  N := 2147483648;
  SetLength(A, N);
end.
