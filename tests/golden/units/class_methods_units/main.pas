program Main;

// Methods across units (objects.md 2.3-2.5): a class of a unit declares its methods in the
// `interface` section and implements them in `implementation`; a class of the program overrides
// a `protected` virtual method of it and calls the parent's with `inherited`.

uses Shapes;
type
  TSq = class(TShape)
  protected
    function Inner: Integer; override;
  end;
function TSq.Inner: Integer;
begin
  Result := inherited Inner * 2;
end;
var S: TShape;
begin
  S := TSq.Create;
  WriteLn(S.Area);
  S.Free;
  S := TShape.Create;
  WriteLn(S.Area);
  S.Free;
end.
