program SetCharParams;

// More sets (types.md 8.3) with results FPC gives too: a set of character codes (a set
// of a `Char` subrange is a set of `AnsiChar` in FPC and does not take a `Char`), sets as
// value, `const` and `var` parameters and as function results, sets in records and arrays, typed constant sets, `Low` / `High` loops with `in`,
// and assignment between set variables that does not share storage.

type
  TDay = (Mon, Tue, Wed, Thu, Fri, Sat, Sun);
  TDays = set of TDay;
  TLetters = set of 97..122;
  TSmall = set of 0..200;
  TPlan = record
    Name: string;
    Days: TDays;
  end;

const
  Weekend: TDays = [TDay.Sat, TDay.Sun];
  Vowels: TLetters = [Ord('a'), Ord('e'), Ord('i'), Ord('o'), Ord('u')];

var
  P: TPlan;
  Week: array[0..1] of TDays;
  A, B: TSmall;
  D: TDay;
  L: TLetters;
  C: Char;
  N: Integer;

function Count(const S: TDays): Integer;
var
  X: TDay;
begin
  Result := 0;
  for X := Low(TDay) to High(TDay) do
    if X in S then
      Inc(Result);
end;

function Workdays: TDays;
begin
  Result := [TDay.Mon..TDay.Sun] - Weekend;
end;

procedure AddDay(var S: TDays; X: TDay);
begin
  S := S + [X];
end;

procedure Show(S: TDays);
var
  X: TDay;
begin
  S := S + [TDay.Mon]; // a value parameter is a copy
  for X in S do
    Write(Ord(X), ' ');
  WriteLn;
end;

begin
  WriteLn(Count(Weekend), ' ', Count(Workdays), ' ', Count([]));
  P.Name := 'gym';
  P.Days := [TDay.Tue, TDay.Thu];
  AddDay(P.Days, TDay.Sat);
  Show(P.Days);
  Show(P.Days);
  WriteLn(P.Name, ' ', Count(P.Days), ' ', TDay.Mon in P.Days);
  Week[0] := Weekend;
  Week[1] := Week[0];
  Include(Week[1], TDay.Fri);
  WriteLn(Count(Week[0]), ' ', Count(Week[1]), ' ', Week[0] <= Week[1], ' ', Week[0] = Week[1]);

  L := [];
  for C in 'hello world' do
    if (C >= 'a') and (C <= 'z') then
      L := L + [Ord(C)];
  for C := 'a' to 'z' do
    if Ord(C) in L then
      Write(C);
  WriteLn;
  for C := 'a' to 'z' do
    if Ord(C) in L * Vowels then
      Write(C);
  WriteLn;
  WriteLn(Ord('x') in L, ' ', Ord('o') in L);

  A := [0, 100..105, 200];
  B := A;
  Exclude(B, 0);
  Include(B, 150);
  N := 0;
  for N in A do
    Write(N, ' ');
  WriteLn;
  for N in B do
    Write(N, ' ');
  WriteLn;
  WriteLn(A - B = [0], ' ', B - A = [150], ' ', A * B = [100..105, 200]);
  for D := TDay.Mon to TDay.Sun do
    if D in Workdays then
      Write(Ord(D));
  WriteLn;
end.
