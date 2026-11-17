program GenericConstructor;

// `T.Create` of a type parameter with the `constructor` constraint (generics.md 3.3, 4.3) calls
// the constructor `Create` the concrete class has without arguments, found as the name is found:
// its own, an ancestor's, one whose parameters all have default values, the built-in
// `TObject.Create`. A virtual constructor is called statically for the concrete class, and
// `AfterConstruction` runs as for `TFoo.Create`, also for a class that is an instantiation made
// while another instantiation's body is copied (`TBox<T>` from `TWrap<T>.Run`). A member can
// follow `T.Create` directly (`T.Create.ClassName`, `T.Create.Free`).

type
  TBase = class
    FTag: string;
    constructor Create; virtual;
  end;

  TDerived = class(TBase)
    constructor Create; override;
  end;

  TInherits = class(TDerived)
  end;

  TDefaults = class
    FA: Integer;
    FB: string;
    constructor Create(A: Integer = 7; const B: string = 'seven');
  end;

  TAfter = class
    procedure AfterConstruction; override;
  end;

  TCounted = class(TRefObject)
  end;

  TFactory<T: class, constructor> = class
    function Make: T;
    procedure Describe;
  end;

  TOwner<T: class, NotRefCounted, constructor> = class
    procedure Churn;
  end;

  TBox<T> = class
    FValue: T;
    procedure AfterConstruction; override;
  end;

  TWrap<T> = class
    procedure Run;
  end;

  TBaseFactory<T: TBase, constructor> = class
    function Make: T;
  end;

constructor TBase.Create;
begin
  FTag := 'base';
end;

constructor TDerived.Create;
begin
  inherited;
  FTag := FTag + '+derived';
end;

constructor TDefaults.Create(A: Integer; const B: string);
begin
  FA := A;
  FB := B;
end;

procedure TAfter.AfterConstruction;
begin
  WriteLn('after construction of ', ClassName);
end;

function TFactory<T>.Make: T;
begin
  Result := T.Create;
end;

procedure TFactory<T>.Describe;
begin
  WriteLn('made a ', T.Create.ClassName);
end;

procedure TOwner<T>.Churn;
begin
  T.Create.Free;
end;

procedure TBox<T>.AfterConstruction;
begin
  WriteLn('after construction of ', ClassName);
end;

procedure TWrap<T>.Run;
var
  F: TFactory<TBox<T>>;
  B: TBox<T>;
begin
  F := TFactory<TBox<T>>.Create;
  B := F.Make;
  WriteLn(B.ClassName);
  B.Free;
  F.Free;
end;

function TBaseFactory<T>.Make: T;
begin
  Result := T.Create;
  WriteLn(Result.ClassName, ': ', Result.FTag);
end;

var
  Objects: TFactory<TObject>;
  Defaults: TFactory<TDefaults>;
  Afters: TFactory<TAfter>;
  Bases: TBaseFactory<TBase>;
  Inherits: TBaseFactory<TInherits>;
  Counted: TFactory<TCounted>;
  Owner: TOwner<TAfter>;
  Wrap: TWrap<Integer>;
  O: TObject;
  D: TDefaults;
  A: TAfter;
  B: TBase;
begin
  Objects := TFactory<TObject>.Create;
  O := Objects.Make;
  WriteLn(O.ClassName);
  Defaults := TFactory<TDefaults>.Create;
  D := Defaults.Make;
  WriteLn(D.FA, ' ', D.FB);
  Afters := TFactory<TAfter>.Create;
  A := Afters.Make;
  Bases := TBaseFactory<TBase>.Create;
  B := Bases.Make;
  B.Free;
  Inherits := TBaseFactory<TInherits>.Create;
  B := Inherits.Make;
  B.Free;
  Counted := TFactory<TCounted>.Create;
  Counted.Describe;
  Counted.Free;
  Owner := TOwner<TAfter>.Create;
  Owner.Churn;
  Owner.Free;
  Wrap := TWrap<Integer>.Create;
  Wrap.Run;
  Wrap.Free;
  O.Free;
  D.Free;
  A.Free;
  Objects.Free;
  Defaults.Free;
  Afters.Free;
  Bases.Free;
  Inherits.Free;
end.
