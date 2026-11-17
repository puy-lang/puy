program DumpInterfaceChecks;

// The dumps of checks with interfaces (objects.md 15.4): `is` / `as` / a conversion by type name
// to an interface (`IsIntf` / `AsIntf` with the interface's type index loaded by `LoadInt`), to a
// class from an interface value (`IsClass` / `AsClass`), `as TObject` (nothing), and the three
// forms of Supports (`supports` with and without its `out` variable, `supports-class` /
// `ClassSupports`).

uses System.SysUtils;

type
  IShape = interface
    function Area: Integer;
  end;

  TSquare = class(TRefObject, IShape)
  public
    function Area: Integer;
  end;

function TSquare.Area: Integer;
begin
  Result := 4;
end;

var
  O: TObject;
  S: IShape;
  Q: TSquare;
begin
  O := TSquare.Create;
  WriteLn(O is IShape);
  S := O as IShape;
  S := IShape(O);
  Q := S as TSquare;
  O := S as TObject;
  WriteLn(Supports(O, IShape, S), ' ', Supports(S, IShape), ' ', Supports(TSquare, IShape));
end.
