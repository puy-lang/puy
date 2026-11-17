unit Shapes;

interface

type
  TShape = class
  private
    Secret: Integer;
  strict protected
    Size: Integer;
  public
    constructor Create;
  end;

implementation

constructor TShape.Create;
begin
  inherited Create;
  Secret := 1;
  Size := 2;
end;

end.
