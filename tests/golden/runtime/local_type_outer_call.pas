program LocalTypeOuterCall;

// A method of a type declared in a function calls the function like any other routine
// (types.md 1.1): the bare name `Run` is a call there, not the recursive-call E0704 and not
// `Result` of `Run`, because the method is not nested in `Run`.

var
  Depth: Integer = 0;

function Run: Integer;
type
  TB = class
    function V: Integer;
  end;

  function TB.V: Integer;
  begin
    Result := Run + 1;
  end;

var
  B: TB;
begin
  Inc(Depth);
  if Depth > 1 then
    Exit(10);
  B := TB.Create;
  Result := B.V;
  B.Free;
end;

begin
  WriteLn(Run);
end.
