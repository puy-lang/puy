program InterfaceCastErrors;

// Checks with interfaces that fail (objects.md 7.6): `as` and the conversions by type name
// `IFoo(Obj)` and `IBar(Intf)` raise EInvalidCast naming the class and the interface (`does not
// implement`), `TFoo(Intf)` and `Intf as TFoo` raise the class check's EInvalidCast (Delphi's
// `TFoo(Intf)` gives `nil`), and `is` / `as` / Supports with a freed instance raise
// EAccessViolation. A variable assigned from a failed check keeps its value.

uses System.SysUtils;

type
  IFoo = interface
    procedure Foo;
  end;

  IBar = interface
    procedure Bar;
  end;

  TFoo = class(TRefObject, IFoo)
  public
    procedure Foo;
  end;

  TOther = class(TRefObject, IFoo)
  public
    procedure Foo;
  end;

  TManual = class(TObject, IFoo)
  public
    procedure Foo;
  end;

procedure TFoo.Foo;
begin
end;

procedure TOther.Foo;
begin
end;

procedure TManual.Foo;
begin
end;

var
  O: TObject;
  F: IFoo;
  B: IBar;
  T: TFoo;
  M: TManual;

procedure Report(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  O := TOther.Create;
  F := TOther.Create;
  try
    B := O as IBar;
  except
    on E: Exception do
      Report(E);
  end;
  try
    B := IBar(O);
  except
    on E: Exception do
      Report(E);
  end;
  try
    B := F as IBar;
  except
    on E: Exception do
      Report(E);
  end;
  try
    B := IBar(F);
  except
    on E: Exception do
      Report(E);
  end;
  try
    T := TFoo(F);
  except
    on E: Exception do
      Report(E);
  end;
  try
    T := F as TFoo;
  except
    on E: Exception do
      Report(E);
  end;
  O := TObject.Create;
  try
    F := O as IFoo;
  except
    on E: Exception do
      Report(E);
  end;
  O.Free;

  // A freed instance
  M := TManual.Create;
  O := M;
  M.Free;
  try
    WriteLn(O is IFoo);
  except
    on E: Exception do
      Report(E);
  end;
  try
    F := O as IFoo;
  except
    on E: Exception do
      Report(E);
  end;
  try
    WriteLn(Supports(O, IFoo, F));
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn(F = nil);
end.
