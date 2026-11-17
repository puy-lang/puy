unit Shapes;
interface
type
  TShape = class
  protected
    function Inner: Integer; virtual;
  public
    function Area: Integer; virtual;
  end;
implementation
function TShape.Inner: Integer;
begin
  Result := 7;
end;
function TShape.Area: Integer;
begin
  Result := Inner;
end;
end.
