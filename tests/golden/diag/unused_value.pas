program UnusedValue;

// W0002: an assignment statement whose value no path reads before the next assignment
// statement to the whole variable or the end of the routine. Each exempt case is marked.

type
  TPoint = record
    X, Y: Integer;
  end;

var
  Global: Integer;

procedure Fill(out X: Integer);
begin
  X := 7;
end;

procedure Bump(var X: Integer);
begin
  Inc(X);
end;

// Reported: overwritten, and still unread when the routine ends
procedure Targets(N: Integer);
var
  A, B: Integer;
begin
  A := 1;
  A := 2;
  WriteLn(A);
  B := N;
  WriteLn(B);
  B := 0;
  N := 5;
end;

// Reported: the value is overwritten on every path through the branches
procedure Branches(Flag: Boolean);
var
  S: string;
begin
  S := 'none';
  if Flag then
    S := 'yes'
  else
    S := 'no';
  WriteLn(S);
end;

// Not reported: read on one path, or read by the next iteration of a loop
procedure Paths(Flag: Boolean; N: Integer);
var
  S: string;
  I, Sum: Integer;
begin
  S := 'none';
  if Flag then
    S := 'yes';
  WriteLn(S);
  Sum := 0;
  I := 0;
  while I < N do
  begin
    Sum := Sum + I;
    I := I + 1;
  end;
  WriteLn(Sum);
end;

// Loops: reported when overwritten within the same iteration; not reported when the next
// iteration or the code after the loop reads the value
procedure InLoops(N: Integer);
var
  X, Prev, Last: Integer;
begin
  Prev := 0;
  for var I := 1 to N do
  begin
    X := I;
    X := I * 2;
    WriteLn(X + Prev);
    Prev := X;
  end;
  Last := 0;
  repeat
    Last := N;
    Dec(N);
  until N <= 0;
  WriteLn(Last);
end;

// Not reported: the exempt cases
function Exempt(var V: Integer; out O: Integer): Integer;
var
  Init: Integer = 1;
  Counted, Passed, Filled, Loop, Part: Integer;
  Line: string;
  R: TPoint;
begin
  // An initial value of a declaration
  Init := 2;
  WriteLn(Init);
  var Other := 3;
  Other := 4;
  WriteLn(Other);
  // `Inc` / `Dec`, `ReadLn`, and `var` / `out` arguments are not assignment statements
  Counted := 0;
  Inc(Counted);
  Dec(Counted);
  Line := 'none';
  ReadLn(Line);
  Passed := 0;
  Bump(Passed);
  Filled := 0;
  Fill(Filled);
  WriteLn(Counted, Line, Passed, Filled);
  // Writing a field reads the whole variable
  R := Default(TPoint);
  R.X := 1;
  // `Result`, `var` / `out` parameters, global variables, `for` loop variables
  Result := 1;
  Result := 2;
  V := 1;
  V := 2;
  O := 1;
  O := 2;
  Global := 1;
  Global := 2;
  Loop := 0;
  for Loop := 1 to 2 do
    WriteLn(Loop);
  // A variable whose value is never read gets W0001 alone
  Part := 1;
  Part := 2;
end;

var
  V, O: Integer;
begin
  Targets(1);
  Branches(True);
  Paths(False, 3);
  InLoops(2);
  V := 0;
  WriteLn(Exempt(V, O), V, O, Global);
end.
