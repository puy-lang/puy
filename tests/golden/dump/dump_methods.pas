program DumpMethods;

// The dumps of methods (objects.md 15.1-15.3): `.method` lines with the table numbers, `Self` as
// the first variable, `method` / `virtual K` / `inherited` calls and `self`, `CallMethod` /
// `CallVirtual` and `.reg self`, and the `vmt=` of the type section. A virtual method called
// from the type of a `sealed` class, and a `final` method, are called with `CallMethod`.

type
  TShape = class
    function Area: Integer; virtual; abstract;
    function Twice: Integer; virtual;
    procedure Show;
  end;

  TSquare = class(TShape)
    Side: Integer;
    function Area: Integer; override;
    function Twice: Integer; override; final;
  end;

  TBox = class sealed(TSquare)
    function Area: Integer; override;
    function ToString: string; override;
  end;

function TShape.Twice: Integer;
begin
  Result := 2 * Area;
end;

procedure TShape.Show;
begin
  WriteLn(Twice, ' ', ToString, ' ', ClassName);
end;

function TSquare.Area: Integer;
begin
  Result := Side * Side;
end;

function TSquare.Twice: Integer;
begin
  Result := inherited Twice + 1;
end;

function TBox.Area: Integer;
begin
  Result := 6 * inherited Area;
end;

function TBox.ToString: string;
begin
  Result := 'box';
end;

var
  S: TShape;
  Q: TSquare;
  B: TBox;
begin
  B := TBox.Create;
  B.Side := 1;
  S := B;
  Q := B;
  S.Show;
  WriteLn(B.Area, ' ', Q.Twice, ' ', S.Area);
  B.Free;
end.
