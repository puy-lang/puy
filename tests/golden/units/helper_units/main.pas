program Main;

// Helpers across units (objects.md 10.4): a helper declared in the `interface` section of a unit
// in `uses` works in the program, also with the unit's name in `H.M(X)`, together with a helper
// of the program for the same type; one declared in `implementation` is only used inside its
// unit.

uses Shouting;

type
  TLocalHelper = record helper for string
    function Double: string;
  end;

function TLocalHelper.Double: string;
begin
  Result := Self + Self;
end;

var
  S: string;
begin
  S := 'hey';
  WriteLn(S.Shout);
  WriteLn(Shouting.TShoutHelper.Shout(S));
  WriteLn(S.Double.Shout);
  WriteLn(Quiet(S));
end.
