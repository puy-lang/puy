program DumpRtti;

// The type information section (rtti.md chapter 7, bytecode.md 3.2, 5.2): the types TypeInfo
// names and those they reach (elements, bases, parents, classes, parameters, type arguments),
// at the identification stage only (no function reads the members), numbered in the order of
// their fully qualified names. Records and classes are in the type section. TypeInfo(T) in a
// generic body is a constant of each instantiation, so those functions are not shared; the
// other methods of the instantiations are

type
  TColor = (Red, Green, Blue);
  TSmall = TColor.Green..TColor.Blue;
  TDigit = 0..9;
  TName = type string;
  TCount = type Byte;
  TColors = set of TColor;
  TGrid = array[1..3] of Double;
  TPoint = record
    X, Y: Integer;
  end;
  TPoints = array[TColor] of TPoint;
  TShape = class
    Name: string;
  end;
  TCircle = class(TShape)
  end;
  TShapeClass = class of TShape;
  IThing = interface
    ['{12345678-1234-1234-1234-123456789ABC}']
    procedure Run;
  end;
  TFn = function(A: Integer; var B: string): Boolean;
  TEvent = procedure(Sender: TObject) of object;
  TRef = reference to function: Integer;
  TBox<T> = class
    function Info: PTypeInfo;
    function Twice(N: Integer): Integer;
  end;

function TBox<T>.Info: PTypeInfo;
begin
  Result := TypeInfo(T);
end;

function TBox<T>.Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

procedure Show(P: PTypeInfo);
begin
  Write(Assigned(P), ' ');
end;

var
  BI: TBox<Integer>;
  BS: TBox<string>;
begin
  Show(TypeInfo(TSmall));
  Show(TypeInfo(TDigit));
  Show(TypeInfo(TName));
  Show(TypeInfo(TCount));
  Show(TypeInfo(TColors));
  Show(TypeInfo(TGrid));
  Show(TypeInfo(TPoints));
  Show(TypeInfo(TCircle));
  Show(TypeInfo(TShapeClass));
  Show(TypeInfo(IThing));
  Show(TypeInfo(TFn));
  Show(TypeInfo(TEvent));
  Show(TypeInfo(TRef));
  Show(TypeInfo(TArray<string>));
  Show(TypeInfo(UTF8String));
  Show(TypeInfo(Currency));
  Show(TypeInfo(Single));
  Show(TypeInfo(Char));
  Show(TypeInfo(UInt64));
  Show(TypeInfo(PTypeInfo));
  WriteLn;
  BI := TBox<Integer>.Create;
  BS := TBox<string>.Create;
  WriteLn(BI.Info <> BS.Info, ' ', BI.Twice(2) + BS.Twice(3));
  BI.Free;
  BS.Free;
end.
