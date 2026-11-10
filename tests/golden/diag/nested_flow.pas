program NestedFlow;

// Definite assignment across calls of nested routines (semantics.md 4.4, 4.9): a call reads the
// enclosing routine's variables the nested routine (and the nested routines it calls) may read
// before assigning them, and assigns those it assigns on every path that returns. Inside the nested
// routine they count as assigned
procedure Calls;
var
  X, Y: Integer;
  S: string;

  procedure Init;
  begin
    X := 0;
  end;

  procedure Show;
  begin
    WriteLn(X);
  end;

  // Assigns S only on one path, then reads Y
  procedure Partly(B: Boolean);
  begin
    if B then
      S := 'a';
    WriteLn(Y);
  end;

begin
  Show;
  Init;
  Show;
  Partly(True);
  Y := 1;
  Partly(False);
  WriteLn(S);
end;

// Through another nested routine: `Both` assigns X before `Show` reads it, so it reads nothing and
// assigns X
procedure Through;
var
  X: Integer;

  procedure Init;
  begin
    X := 0;
  end;

  procedure Show;
  begin
    WriteLn(X);
  end;

  procedure Both;
  begin
    Init;
    Show;
  end;

begin
  Both;
  WriteLn(X);
end;

// Mutually recursive nested routines: what they read and assign is worked out together
function Mutual(N: Integer): Integer;
var
  A: Integer;

  procedure Odd(K: Integer); forward;

  procedure Even(K: Integer);
  begin
    if K = 0 then
      A := 1
    else
      Odd(K - 1);
  end;

  procedure Odd(K: Integer);
  begin
    if K = 0 then
      A := 2
    else
      Even(K - 1);
  end;

begin
  Even(N);
  Result := A;
end;

// A call through the out parameter of an enclosing routine
procedure Outs(out R: Integer);
  procedure SetR;
  begin
    R := 5;
  end;
begin
  SetR;
end;

var
  R: Integer;
begin
  Calls;
  Through;
  WriteLn(Mutual(3));
  Outs(R);
  WriteLn(R);
end.
