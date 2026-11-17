program AnonInheritedClassName;

// An anonymous method in a method can call `inherited Name` (it captures `Self`; objects.md 9.6),
// and one in a class method uses the class reference as `Self` (`ClassName` is the class the
// class method was called through).

type
  TBase = class(TRefObject)
    procedure Hello; virtual;
    class function Describe: TProc;
  end;

  TChild = class(TBase)
    procedure Hello; override;
  end;

procedure TBase.Hello;
begin
  WriteLn('base hello');
end;

class function TBase.Describe: TProc;
begin
  Result := procedure
    begin
      WriteLn('class ', ClassName);
    end;
end;

procedure TChild.Hello;
var
  P: TProc;
begin
  P := procedure
    begin
      inherited Hello;
      WriteLn('child hello');
    end;
  P();
end;

var
  C: TBase;
  D: TProc;
begin
  C := TChild.Create;
  C.Hello;
  D := TBase.Describe();
  D();
  D := TChild.Describe();
  D();
end.
