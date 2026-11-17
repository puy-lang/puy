program InterfaceCasts;

// Checks with interfaces that hold (objects.md 7.3, 7.6): `as`, `is` and Supports find an
// ancestor of an interface a class lists (Delphi only finds the listed ones), the conversions by
// type name `IFoo(Obj)`, `TFoo(Intf)` and `IBar(Intf)` pass when the instance fits, `is` /
// `as` / a conversion from an interface value to a class look at the instance's class, `as
// TObject` always holds, and `nil` gives `False` (Supports writes `nil` to its `out` variable).

uses System.SysUtils;

type
  IBase = interface
    function Name: string;
  end;

  IDerived = interface(IBase)
    procedure Bump;
  end;

  IOther = interface
    procedure Other;
  end;

  TImpl = class(TRefObject, IDerived)
  private
    FCount: Integer;
  public
    function Name: string;
    procedure Bump;
  end;

  TImplChild = class(TImpl)
  end;

  TManual = class(TObject, IBase)
  public
    function Name: string;
  end;

function TImpl.Name: string;
begin
  Result := 'impl ' + IntToStr(FCount);
end;

procedure TImpl.Bump;
begin
  Inc(FCount);
end;

function TManual.Name: string;
begin
  Result := 'manual';
end;

var
  O: TObject;
  Base: IBase;
  Derived: IDerived;
  Other: IOther;
  Impl: TImpl;
  M: TManual;
begin
  // An ancestor of the listed interface
  O := TImplChild.Create;
  Base := O as IBase;
  WriteLn(Base.Name, ' ', O is IBase, ' ', O is IDerived, ' ', O is IOther);
  WriteLn(Supports(O, IBase), ' ', Supports(TImpl, IBase), ' ', Supports(TImplChild, IInterface));

  // Conversions by type name that fit
  Derived := IDerived(Base);
  Derived.Bump;
  Base := IBase(O);
  WriteLn(Base.Name);
  Impl := TImpl(Base);
  Impl.Bump;
  WriteLn(TImplChild(Derived).Name, ' ', IBase(Impl).Name);

  // From an interface value to a class
  WriteLn(Base is TImpl, ' ', Base is TImplChild, ' ', Base is TManual, ' ', Base is TObject);
  WriteLn((Base as TImplChild).Name, ' ', (Base as TObject).ClassName);
  WriteLn(Derived is IBase, ' ', Base is IDerived, ' ', Base is IOther);

  // A class managed by hand
  M := TManual.Create;
  Base := M;
  WriteLn((Base as TManual).Name, ' ', Supports(M, IDerived), ' ', M is IBase);
  Base := nil;
  M.Free;

  // `nil`
  O := nil;
  Base := nil;
  Other := nil;
  Derived := TImpl.Create;
  WriteLn(O is IBase, ' ', Base is IDerived, ' ', Base is TImpl, ' ', Supports(O, IBase));
  WriteLn(Supports(O, IDerived, Derived), ' ', Derived = nil);
  Derived := TImpl.Create;
  WriteLn(Supports(Derived, IOther, Other), ' ', Other = nil);
  WriteLn(O as IBase = nil, ' ', Base as TImpl = nil, ' ', IDerived(Base) = nil, ' ', TImpl(Base) = nil);
  Impl := nil;
  WriteLn(Supports(Impl, IBase), ' ', IBase(Impl) = nil);
end.
