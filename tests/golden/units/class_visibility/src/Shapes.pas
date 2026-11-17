unit Shapes;

interface

type
  TShape = class
  private
    FName: string;
  strict private
    FSecret: Integer;
  protected
    FSize: Integer;
  public
    Kind: Integer;
  end;

function MakeShape: TShape;

implementation

function MakeShape: TShape;
begin
  Result := TShape.Create;
  // `private` and `protected` reach the whole unit; `strict private` does not (objects.md 2.5)
  Result.FName := 'shape';
  Result.FSize := 2;
end;

end.
