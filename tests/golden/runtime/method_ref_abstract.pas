program MethodRefAbstract;

// A method reference to an abstract method of an instance whose class does not override it
// (created through a class reference) raises `EAbstractError` where it is made (objects.md 9.3,
// 3.1), and the program ends.

type
  TEvent = procedure of object;

  TShape = class(TRefObject)
    procedure Draw; virtual; abstract;
  end;

  TShapeClass = class of TShape;

var
  K: TShapeClass;
  S: TShape;
  E: TEvent;
begin
  K := TShape;
  S := K.Create;
  WriteLn('created');
  E := S.Draw;
  WriteLn('not reached');
end.
