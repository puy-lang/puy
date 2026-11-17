program InterfaceAccessErrors;

// A call through an interface checks the instance (objects.md 7.5, 14.4): `nil` and an instance
// of a class managed by hand freed through a class variable are EAccessViolation, and an abstract
// method implementing an interface's method is EAbstractError naming the interface (an instance
// of an abstract class made through a class reference; objects.md 3.1, 7.3).

uses System.SysUtils;

type
  IRun = interface
    procedure Run;
  end;

  TManual = class(TObject, IRun)
  public
    procedure Run;
  end;

  TAbstractRun = class(TRefObject, IRun)
  public
    procedure Run; virtual; abstract;
  end;

  TRunClass = class of TAbstractRun;

procedure TManual.Run;
begin
  WriteLn('run');
end;

procedure Call(const R: IRun);
begin
  try
    R.Run;
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end;

var
  M: TManual;
  R: IRun;
  C: TRunClass;
begin
  M := TManual.Create;
  R := M;
  Call(R);
  M.Free;
  Call(R);
  R := nil;
  Call(R);
  C := TAbstractRun;
  R := C.Create;
  Call(R);
end.
