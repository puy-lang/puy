program DumpInlineCopies;

{$Q-}
// The copies and tests left where routines are inlined (design/inlining.md 7.2): the readers of
// an argument copied before the routine's branches read the caller's value (rule 13); a Boolean
// result tested again after the call, and the second test of `and`, go straight where the first
// test leads, and a compare whose result only a test reads becomes a compare-and-branch
// (rule 14); a result copied into a call's argument is computed in the argument's register
// (rule 15).

type
  TRange = class
    Low, High: Integer;
    function Find(K: Integer; out V: Integer): Boolean;
    function Has(K: Integer): Boolean;
  end;

function TRange.Find(K: Integer; out V: Integer): Boolean;
begin
  Result := (K >= Low) and (K <= High);
  if Result then
    V := K - Low
  else
    V := -1;
end;

function TRange.Has(K: Integer): Boolean;
begin
  Result := K - Low >= 0;
end;

function Clamp(X: Integer): Integer;
begin
  if X < 0 then
    Result := 0
  else
    Result := X;
end;

function Count(N: Integer): Integer;
begin
  if N <= 0 then
    Result := 0
  else
    Result := 1 + Count(N - 1);
end;

procedure Run(R: TRange; A, B: Integer);
var
  V: Integer;
begin
  if R.Find(A, V) then
    WriteLn(V);
  if R.Has(B) then
    WriteLn('has');
  WriteLn(Count(Clamp(A - B)));
end;

var
  R: TRange;

begin
  R := TRange.Create;
  R.Low := 1;
  R.High := 5;
  Run(R, 3, 2);
  Run(R, 7, 9);
  R.Free;
end.
