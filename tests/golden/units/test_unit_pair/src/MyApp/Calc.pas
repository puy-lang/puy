// The unit under test: its `implementation` section, and the `private` and `protected` members of
// its types, are seen by `MyApp.Calc_test` (units.md 2.4).
unit MyApp.Calc;

interface

type
  TCounter = class
  private
    FCount: Integer;
  protected
    procedure Bump;
  public
    function Count: Integer;
  end;

  TPoint = record
  private
    X: Integer;
  public
    function GetX: Integer;
  end;

function Add(A, B: Integer): Integer; overload;

implementation

uses
  MyApp.Store;

const
  Offset = 10;

type
  TBox<T> = record
    Value: T;
  end;

  TScratch = class
  private
    FNote: string;
  end;

var
  Calls: Integer;

function Add(A, B: Integer): Integer;
begin
  Inc(Calls);
  Result := A + B;
end;

function Add(const A, B: string): string; overload;
begin
  Result := A + B;
end;

function Shifted(N: Integer): Integer;
begin
  Result := N + Offset + Stored;
end;

procedure TCounter.Bump;
begin
  Inc(FCount);
end;

function TCounter.Count: Integer;
begin
  Result := FCount;
end;

function TPoint.GetX: Integer;
begin
  Result := X;
end;

end.
