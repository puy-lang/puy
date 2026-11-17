program AnonSharedVars;

// Anonymous methods capture variables, not values (objects.md 9.6): an anonymous method and the
// routine around it share the variable, so each sees what the other wrote; a counter made by a
// function keeps its variable after the function returns, and two counters have two variables;
// a value parameter and a string are captured the same way; a loop variable declared before the
// loop is one variable, read when the anonymous method is called; `TProc` is the
// `reference to procedure` of `System`; `Assigned` and `nil` work as for other procedural values.

type
  TCounter = reference to function: Integer;
  TIntProc = reference to procedure(N: Integer);

function MakeCounter(Start: Integer): TCounter;
begin
  Result := function: Integer
    begin
      Inc(Start);
      Result := Start;
    end;
end;

procedure Shared;
var
  X: Integer;
  S: string;
  Show: TProc;
  SetX: TIntProc;
begin
  X := 1;
  S := 'a';
  Show := procedure
    begin
      WriteLn('X = ', X, ', S = ', S);
    end;
  SetX := procedure(N: Integer)
    begin
      X := N;
      S := S + 'b';
    end;
  Show();
  X := 2;
  Show();
  SetX(10);
  WriteLn('outside: ', X, ' ', S);
  Show();
end;

procedure LoopVar;
var
  I: Integer;
  P: TProc;
begin
  I := 0;
  P := procedure
    begin
      WriteLn('I = ', I);
    end;
  for I := 1 to 3 do
    P();
end;

var
  A, B: TCounter;
  P: TProc;
begin
  Shared;
  A := MakeCounter(0);
  B := MakeCounter(100);
  WriteLn(A(), ' ', A(), ' ', B(), ' ', A(), ' ', B());
  LoopVar;
  P := nil;
  WriteLn(Assigned(P));
  P := procedure begin WriteLn('called') end;
  WriteLn(Assigned(P), ' ', P <> nil);
  P();
  P := nil;
  WriteLn(Assigned(P));
end.
