program InlineCopies;

{$Q+}
// The copies and tests left where routines are inlined go (design/inlining.md 7.2): a Boolean
// result tested again after the call, a compare whose result only a test reads, an argument
// copied into the registers an instruction wants in a row, a `for` limit copied before the loop.
// What the program does stays: the limit is read once, an `out` argument is written through its
// reference, an overflow in an inlined routine fails at its own line, and managed values are
// released as before.

uses System.SysUtils, System.Generics.Collections;

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

function Add(X, Y: Integer): Integer;
begin
  Result := X + Y;
end;

function Twice(X: Integer): Integer;
begin
  Result := Add(X, X);
end;

procedure Limit;
var
  I, N, S: Integer;
begin
  N := 3;
  S := 0;
  for I := 1 to N do
  begin
    N := N + 10;
    S := S + I;
  end;
  WriteLn('limit read once: ', S, ' ', N);
end;

procedure Lookups(R: TRange);
var
  I, V, Hits, Sum: Integer;
begin
  Hits := 0;
  Sum := 0;
  for I := 0 to 9 do
  begin
    if R.Find(I, V) then
      Sum := Sum + V * 10
    else
      Sum := Sum + V;
    if R.Has(I) then
      Hits := Hits + 1;
    if not R.Has(I + 1) then
      Hits := Hits + 100;
  end;
  WriteLn('lookups: ', Hits, ' ', Sum);
end;

procedure Strings;
var
  D: TDictionary<string, Integer>;
  I, V, Sum: Integer;
  Key: string;
begin
  D := TDictionary<string, Integer>.Create;
  try
    for I := 0 to 19 do
      D.Add('k' + IntToStr(I), I);
    Sum := 0;
    for I := 0 to 39 do
    begin
      Key := 'k' + IntToStr(I);
      if D.TryGetValue(Key, V) then
        Sum := Sum + V;
      if D.ContainsKey(Key + '0') then
        Sum := Sum + 1000;
    end;
    WriteLn('strings: ', Sum, ' ', D.Count);
  finally
    D.Free;
  end;
end;

procedure Overflow;
begin
  try
    WriteLn(Twice(MaxInt));
  except
    on E: EIntOverflow do
    begin
      WriteLn(E.ClassName, ': ', E.Message);
      WriteLn(E.StackTrace);
    end;
  end;
end;

var
  R: TRange;

begin
  Limit;
  R := TRange.Create;
  R.Low := 3;
  R.High := 6;
  Lookups(R);
  R.Free;
  Strings;
  Overflow;
  WriteLn(Twice(MaxInt div 2 + 1));
end.
