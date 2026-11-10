program ForInArrays;

// `for..in` visits the elements of dynamic, static and open arrays in ascending index order,
// and the `Char`s of a string from the start (types.md chapter 5).

type
  TInts = array of Integer;
  TWords = array[5..7] of string;
  TLetters = array['a'..'c'] of Integer;
  TPoint = record
    X, Y: Integer;
  end;
  TPoints = array of TPoint;

procedure ShowOpen(const A: array of Integer);
var
  E: Integer;
begin
  Write('open:');
  for E in A do
    Write(' ', E);
  WriteLn;
end;

procedure ShowValue(A: array of string);
var
  S: string;
begin
  Write('value:');
  for S in A do
    Write(' ', S);
  WriteLn;
end;

var
  D: TInts;
  W: TWords;
  L: TLetters;
  P: TPoints;
  Pt: TPoint;
  I: Integer;
  Big: Int64;
  S: string;
  C: Char;
  Count: Integer;

begin
  SetLength(D, 4);
  for I := 0 to 3 do
    D[I] := (I + 1) * 10;
  Write('dynamic:');
  for I in D do
    Write(' ', I);
  WriteLn;

  W[5] := 'five';
  W[6] := 'six';
  W[7] := 'seven';
  Write('static:');
  for S in W do
    Write(' ', S);
  WriteLn;

  L['a'] := 1;
  L['b'] := 2;
  L['c'] := 3;
  Write('char index:');
  for I in L do
    Write(' ', I);
  WriteLn;

  ShowOpen(D);
  ShowOpen(L);
  ShowValue(W);

  SetLength(P, 2);
  P[0].X := 1;
  P[0].Y := 2;
  P[1].X := 3;
  P[1].Y := 4;
  Write('records:');
  for Pt in P do
    Write(' (', Pt.X, ',', Pt.Y, ')');
  WriteLn;

  // An element is assigned to a wider loop variable
  Write('widened:');
  for Big in D do
    Write(' ', Big * 1000000000);
  WriteLn;

  Write('string:');
  for C in 'héllo' do
    Write(' ', Ord(C));
  WriteLn;

  S := '';
  for C in 'abc' do
    S := C + S;
  WriteLn('reversed: ', S);

  // Empty collections run the body zero times
  Count := 0;
  SetLength(D, 0);
  for I in D do
    Inc(Count);
  S := '';
  for C in S do
    Inc(Count);
  WriteLn('empty: ', Count);
end.
