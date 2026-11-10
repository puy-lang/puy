program CaseFlow;

// Control-flow analysis of `case` (semantics.md 4.4, 4.6, 4.10): without `else`, a variable
// assigned in every arm is not definitely assigned even if the labels cover every value (E2000);
// with `else`, it is when every arm and the `else` part assign it. W0003 applies inside each arm
// and the `else` part.

function AllValues(B: Boolean): Integer;
var
  X: Integer;
begin
  case B of
    False: X := 0;
    True: X := 1;
  end;
  Result := X;
end;

function WithElse(I: Integer): Integer;
var
  X: Integer;
begin
  case I of
    1: X := 10;
    2, 3: X := 20;
  else
    X := 0;
  end;
  Result := X;
end;

function MissingInElse(I: Integer): Integer;
var
  X: Integer;
begin
  case I of
    1: X := 10;
  else
    WriteLn('none');
  end;
  Result := X;
end;

function ExitInArms(I: Integer): Integer;
begin
  case I of
    1:
      begin
        Exit(1);
        WriteLn('never');
      end;
    2: Result := 2;
  else
    Result := 0;
    Exit;
    WriteLn('never');
  end;
end;

begin
  WriteLn(AllValues(True), WithElse(1), MissingInElse(1), ExitInArms(1));
end.
