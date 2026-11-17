program GenericConstraintLaterClass;

// The constraints of the instantiations written in a `type` section are checked once the
// section's classes have their parents, interfaces and methods (generics.md 4.2): a type
// argument can name a class declared after it in the section, for an interface, a class type
// and a `constructor` constraint.

type
  IName = interface
    function Name: string;
  end;

  TBase = class
    function Kind: string; virtual;
  end;

  TNamed<T: IName> = class
    function Show(const X: T): string;
  end;

  TKinds<T: TBase> = class
    function Show(const X: T): string;
  end;

  TMaker<T: class, constructor> = class
    function Make: T;
  end;

  TUser = class
    FNamed: TNamed<TImpl>;
    FKinds: TKinds<TDerived>;
    FMaker: TMaker<TDerived>;
  end;

  TImpl = class(TRefObject, IName)
    function Name: string;
  end;

  TDerived = class(TBase)
    constructor Create;
    function Kind: string; override;
  end;

function TBase.Kind: string;
begin
  Result := 'base';
end;

function TNamed<T>.Show(const X: T): string;
begin
  Result := X.Name;
end;

function TKinds<T>.Show(const X: T): string;
begin
  Result := X.Kind;
end;

function TMaker<T>.Make: T;
begin
  Result := T.Create;
end;

function TImpl.Name: string;
begin
  Result := 'impl';
end;

constructor TDerived.Create;
begin
  WriteLn('created');
end;

function TDerived.Kind: string;
begin
  Result := 'derived';
end;

var
  U: TUser;
  D: TDerived;
begin
  U := TUser.Create;
  U.FNamed := TNamed<TImpl>.Create;
  U.FKinds := TKinds<TDerived>.Create;
  U.FMaker := TMaker<TDerived>.Create;
  WriteLn(U.FNamed.Show(TImpl.Create));
  D := U.FMaker.Make;
  WriteLn(U.FKinds.Show(D));
  D.Free;
  U.FMaker.Free;
  U.FKinds.Free;
  U.FNamed.Free;
  U.Free;
end.
