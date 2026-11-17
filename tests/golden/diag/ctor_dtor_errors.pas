program CtorDtorErrors;

// Diagnostics of constructors and destructors (objects.md 3.1, 3.2, 16): E0747 for a
// constructor called on an instance (with the notes), for `Create(…)` without a class inside a
// constructor, and for a destructor not declared as `destructor Destroy; override;` (another
// name, parameters, `virtual`, no `override`) and for one outside a class; E0711 for `Exit` with a value in a constructor and
// a destructor; E0723 for a body whose kind does not match the declaration; E0743 for a
// constructor the code cannot see (it does not fall back to `TObject.Create`); a virtual constructor is
// accepted (M3-29).

type
  TItem = class
  public
    Value: Integer;
    constructor Create(AValue: Integer); overload;
    constructor Create; overload;
    constructor CreateVirtual; virtual;
    destructor Done; override;
    destructor Destroy(Force: Boolean); override;
    procedure Reset;
  end;

  TOther = class
    destructor Destroy; virtual;
  end;

  TPlain = class
    destructor Destroy;
  end;

  TGood = class
    constructor Create;
    destructor Destroy; override;
  end;

  TSecret = class
  strict private
    constructor Create(AValue: Integer);
  end;

constructor TItem.Create(AValue: Integer);
begin
  Value := AValue;
end;

constructor TItem.Create;
begin
  Create(0);
end;

constructor TItem.CreateVirtual;
begin
  Exit(1);
end;

destructor TItem.Done;
begin
end;

destructor TItem.Destroy(Force: Boolean);
begin
end;

procedure TItem.Reset;
begin
  Self.Create(1);
end;

destructor TOther.Destroy;
begin
end;

destructor TPlain.Destroy;
begin
end;

procedure TGood.Create;
begin
end;

destructor TGood.Destroy;
begin
  Exit(1);
end;

constructor TSecret.Create(AValue: Integer);
begin
end;

constructor Orphan;
begin
end;

var
  X: TItem;
begin
  X := TItem.Create(1);
  X.Create(2);
  X.Free;
  TSecret.Create(3);
end.
