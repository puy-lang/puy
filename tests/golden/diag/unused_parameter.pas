program UnusedParameter;

// W0017 is disabled by default; `{$WARN unused-parameter ON}` enables it from that point.

procedure Before(N: Integer);
begin
end;

{$WARN unused-parameter ON}

procedure Later(A: Integer; const C: string; var V: Integer; out O: Integer);
begin
  O := 0;
end;

// Read, or (for a `var` parameter) written, parameters are not reported
procedure Used(A: Integer; const C: string; var V: Integer);
begin
  V := A;
  WriteLn(C);
end;

// Reported at the parameter of the body's heading, not the `forward` declaration
procedure Declared(X: Integer); forward;

procedure Declared(X: Integer);
begin
end;

// A value parameter that is assigned but never read gets W0017 alone where W0017 is enabled,
// as an unread local variable gets W0001 alone
procedure Assigned(N: Integer);
begin
  N := 5;
end;

{$WARN unused-parameter OFF}

// Where W0017 is disabled, the assignment gets W0002
procedure AssignedOff(N: Integer);
begin
  N := 5;
end;

var
  V, O: Integer;
begin
  Before(1);
  V := 0;
  Later(1, 'a', V, O);
  Used(1, 'b', V);
  Declared(2);
  Assigned(3);
  AssignedOff(4);
end.
