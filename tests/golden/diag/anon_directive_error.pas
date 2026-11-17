program AnonDirectiveError;

// An anonymous method cannot have directives (objects.md 9.1): an identifier after its
// parameters where the body starts is E0400 (expected `begin`).

type
  TIntProc = reference to procedure(X: Integer);

var
  R: TIntProc;
begin
  R := procedure(X: Integer) inline; begin end;
end.
