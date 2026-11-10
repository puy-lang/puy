unit Greeting;

interface

const
  Excl = '!';

type
  TPoint = record
    X, Y: Integer;
  end;

var
  Count: Integer = 10;

function Greet(const Name: string): string;
function Origin: TPoint;

implementation

var
  Calls: Integer;

function Greet(const Name: string): string;
begin
  Inc(Calls);
  Result := 'Hello, ' + Name + Excl;
end;

function Origin: TPoint;
begin
  Result := Default(TPoint);
  Result.X := 0;
  Result.Y := Calls;
end;

end.
