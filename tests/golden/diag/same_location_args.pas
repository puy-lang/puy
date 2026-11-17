program SameLocationArgs;

// E0741: the same location passed to two `var` / `out` parameters, when it is known at compile
// time (types.md chapter 3): the same variable, or the same path whose indices are equal
// constants or the same variable. The hint mentions `Exchange` for two array elements.
// Different fields, different variables as indices and a field of a variable passed with the
// variable itself are not reported.

type
  TPair = record
    X, Y: Integer;
  end;

var
  G: Integer;
  A: array of Integer;
  S: array[1..3] of Integer;
  P: TPair;

procedure Swap(var X, Y: Integer);
begin
end;

procedure Two(var R: TPair; out N: Integer);
begin
  N := 0;
end;

procedure Run(var V: Integer);
var
  I, J: Integer;
begin
  I := 0;
  J := 0;
  Swap(G, G);
  Swap(V, V);
  Swap(A[0], A[0]);
  Swap(A[I], A[I]);
  Swap(S[1], S[1]);
  Swap(P.X, P.X);
  Swap(A[I], A[J]);
  Swap(P.X, P.Y);
  Two(P, P.X);
end;

begin
  Run(G);
end.
