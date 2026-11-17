unit Geometry;

interface

uses Log;

type
  TKind = (Square, Wide, Tall);
  TRect = record
    W, H: Integer;
  end;

const
  Unit_Size = 10;
  Origin: TRect = (W: 0; H: 0);

var
  Kinds: Integer;
  Last: TRect;

function MakeRect(W, H: Integer): TRect;
function Area(const R: TRect): Integer;
function Perimeter(const R: TRect): Integer;
function KindOf(const R: TRect): TKind;
function UnitRect: TRect;

const
  DefaultKind = TKind.Wide;

implementation

uses Shapes;

function Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

function MakeRect(W, H: Integer): TRect;
begin
  Result := Origin;
  Result.W := W;
  Result.H := H;
end;

function Area(const R: TRect): Integer;
begin
  Result := R.W * R.H;
end;

function Perimeter(const R: TRect): Integer;
begin
  Result := Twice(R.W + R.H);
end;

function KindOf(const R: TRect): TKind;
begin
  if R.W = R.H then
    Result := TKind.Square
  else if R.W > R.H then
    Result := TKind.Wide
  else
    Result := TKind.Tall;
end;

function UnitRect: TRect;
begin
  Result := MakeRect(Unit_Size, Unit_Size + 1);
end;

initialization
  Note('init Geometry');
  Kinds := Ord(High(TKind)) + 1;

finalization
  Note('fini Geometry');
  Flush(Output); // FPC 3.2.2 does not write out what finalization writes without it
end.
