program InterfaceImplErrors;

// A class implementing interfaces (objects.md 7.3, 7.4, 16.1): a method of an interface the class
// has no method for is E0749 (one per method, at the class); names in parentheses that are not
// interfaces and an interface listed twice are E0750; a resolution clause names an interface the
// class implements, a method of it and a method of the class (an error in a clause for an
// ancestor interface's method is reported once). An interface's value converts to a
// class only with `as` (E0702), a class that does not implement an interface does not convert to
// it, and the value has only the interface's members (E0700). Raising an interface's value is
// E3021; removed names are E3020 / E3010, and `TGUID` is planned for M5.

type
  IShape = interface
    function Area: Integer;
    function Perimeter: Integer;
  end;

  IRun = interface
    procedure Run;
  end;

  TPartial = class(TRefObject, IShape)
  public
    function Area: Integer;
  end;

  TWrongSig = class(TRefObject, IShape, IRun)
  public
    function Area(X: Integer): Integer;
    function Perimeter: Double;
    class procedure Run;
  end;

  TListed = class(TRefObject, IRun, IRun, TObject)
  public
    procedure Run;
  end;

  TClauses = class(TRefObject, IRun)
  public
    procedure IRun.Run = Missing;
    procedure IShape.Area = Run;
    procedure IRun.Stop = Run;
    procedure INone.Run = Run;
    procedure Run;
  end;

  IRun2 = interface(IRun)
    procedure Stop;
  end;

  TAncestorClause = class(TRefObject, IRun2)
  public
    procedure IRun.Run = Missing;
    procedure Stop;
  end;

  TSealed = class sealed(TRefObject)
  end;

  TAgg = class(TAggregatedObject)
  end;

function TPartial.Area: Integer; begin Result := 0; end;
function TWrongSig.Area(X: Integer): Integer; begin Result := X; end;
function TWrongSig.Perimeter: Double; begin Result := 0; end;
class procedure TWrongSig.Run; begin end;
procedure TListed.Run; begin end;
procedure TClauses.Run; begin end;
procedure TAncestorClause.Stop; begin end;

var
  S: IShape;
  R: IRun;
  O: TObject;
  L: TListed;
  T: TSealed;
  D: IDispatch;
  G: TGUID;
begin
  O := R;
  L := R;
  R := O;
  R := T;
  WriteLn(R);
  R.ClassName;
  R.Free;
  WriteLn(S < S);
  raise R;
end.
