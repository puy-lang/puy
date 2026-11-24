unit App.Shapes;

interface

type
  TShape = class
  strict private
    FSecret: Integer;
  strict protected
    procedure Hidden;
  private
    FSide: Integer;
  end;

implementation

uses
  App.Log;

procedure TShape.Hidden;
begin
  FSecret := Logged;
end;

end.
