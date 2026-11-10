program ForInErrors;

// Diagnostics of `for..in` (types.md chapter 5): a collection that is not an array or a
// string (E0730), an element that is not assignment compatible with the loop variable (E0702),
// writing the loop variable in the body (E0707), reading it after the loop (E2004), and the
// loop variable rules shared with `for..to` (E0722, E0700 with the `var` fix).

type
  TInts = array of Integer;
  TPoint = record
    X: Integer;
  end;

var
  G: Integer;
  Ints: TInts;

procedure Bad(var V: Integer; const K: Integer; P: TPoint);
var
  I: Integer;
  S: string;
  B: Byte;
begin
  for I in 10 do
    WriteLn(I);
  for I in P do
    WriteLn(I);
  for S in Ints do
    WriteLn(S);
  for I in 'abc' do
    WriteLn(I);
  for V in Ints do
    WriteLn(V);
  for K in Ints do
    WriteLn(K);
  for G in Ints do
    WriteLn(G);
  for I in Ints do
    I := 0;
  for I in Ints do
    Inc(I);
  for I in Ints do
    for I in Ints do
      WriteLn(I);
  for B in Ints do
    WriteLn(B);
  WriteLn(B);
  for Undeclared in Ints do
    WriteLn(Undeclared);
end;

begin
  Bad(G, 1, Default(TPoint));
  for var X in Ints do
    X := 1;
  WriteLn(X);
end.
