program StaticArrayCtor;

// An array constructor also makes a static array with the same number of elements, an
// inline variable takes it with its type written, and a typed constant of a dynamic array is
// a new array each time it is read, so writing to a copy leaves the constant alone
// (types.md chapter 4; Delphi shares one array).

type
  TTriple = array[0..2] of string;
  TInts = array of Integer;

const
  Primes: TInts = [2, 3, 5];

var
  T: TTriple;
  S: string;
  B: TInts;
begin
  S := 'b';
  T := ['a', S, S + 'c'];
  WriteLn(T[0], T[1], T[2]);
  T := ['x', 'y', 'z'];
  WriteLn(T[0], T[1], T[2]);
  var C: TInts := [9, 8];
  WriteLn(C[0], C[1]);
  B := Primes;
  B[0] := 1;
  WriteLn(B[0], ' ', Primes[0]);
  WriteLn(nil = B, ' ', nil <> C);
end.
