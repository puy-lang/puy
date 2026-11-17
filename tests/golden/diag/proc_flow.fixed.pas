program ProcFlow;

// Definite assignment of procedural variables (objects.md 9.4, semantics.md 4.4): calling a
// local procedural value before it is assigned is E2000, for a procedural type and a method
// reference type alike, and the fix gives the variable the initial value `nil`.

type
  TFn = function(X: Integer): Integer;
  TEvent = procedure of object;

procedure Run;
var
  F: TFn = nil;
  E: TEvent = nil;
begin
  WriteLn(F(1));
  E;
end;

begin
  Run;
end.
