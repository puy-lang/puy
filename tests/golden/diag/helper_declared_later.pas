program HelperDeclaredLater;

// A helper is visible only after its declaration in the same file (objects.md 10.4, 10.10): a
// routine before a local `type` section with the helper cannot use it, a routine after it can.

procedure Before;
var
  S: string;
begin
  S := 'a';
  WriteLn(S.Shout);                                // E3200: no helper here yet
end;

type
  TShout = record helper for string
    function Shout: string;
  end;

function TShout.Shout: string;
begin
  Result := Self + '!';
end;

procedure After;
var
  S: string;
begin
  S := 'b';
  WriteLn(S.Shout);
end;

begin
  Before;
  After;
end.
