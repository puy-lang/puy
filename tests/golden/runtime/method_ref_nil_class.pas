program MethodRefNilClass;

// A method reference to a class method made from a `nil` class reference raises
// `EAccessViolation` where it is made (objects.md 9.3), and the program ends.

type
  TClassFn = function: string of object;

  TThing = class
    class function Name: string;
  end;

  TThingClass = class of TThing;

class function TThing.Name: string;
begin
  Result := 'thing';
end;

var
  K: TThingClass;
  E: TClassFn;
begin
  K := TThing;
  E := K.Name;
  WriteLn(E());
  K := nil;
  E := K.Name;
  WriteLn('not reached');
end.
