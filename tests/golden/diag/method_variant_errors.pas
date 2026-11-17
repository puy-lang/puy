program MethodVariantErrors;

// Variants of the method diagnostics not covered by golden/diag/method_errors (objects.md 1, 2.3,
// 2.4, 2.10, 3.1, 3.2): `dynamic; override;` names the word written; `static` on a constructor and
// a destructor; an `override` whose ancestor has the name with other parameters or another kind
// (with the ancestor's declaration); W0021 suggesting only `reintroduce` when the parameters
// differ; a method that is not `overload` hiding the parent's; E0742 for a `class abstract` class
// (no secondary) and for an abstract method inherited from an ancestor; a destructor outside a
// class; an `overload` method whose body leaves out the parameters; `inherited` alone in a routine
// nested in a method; `UnitName` and `QualifiedClassName`.

type
  TBase = class
    procedure Run(X: Integer); virtual;
    procedure Walk; virtual;
    procedure Add(X: Integer);
    function Area: Integer; virtual; abstract;
  end;

  TChild = class(TBase)
    procedure Walk; dynamic; override;
    constructor Make; static;
    destructor Destroy; override; static;
    procedure Run(S: string); override;
  end;

  TKind = class(TBase)
    class procedure Run(X: Integer); override;
  end;

  TOther = class(TBase)
    procedure Run(S: string);
    procedure Add(S: string);
    function Area: Integer; override;
  end;

  TPlain = class abstract
  end;

  TLeaf = class(TBase)
    constructor Make;
  end;

  TOver = class
    procedure Go(X: Integer); overload;
    procedure Step;
  end;

procedure TBase.Run(X: Integer);
begin
end;

procedure TBase.Walk;
begin
end;

procedure TBase.Add(X: Integer);
begin
end;

procedure TChild.Walk;
begin
end;

constructor TChild.Make;
begin
end;

destructor TChild.Destroy;
begin
end;

procedure TChild.Run(S: string);
begin
end;

class procedure TKind.Run(X: Integer);
begin
end;

procedure TOther.Run(S: string);
begin
end;

procedure TOther.Add(S: string);
begin
end;

function TOther.Area: Integer;
begin
  Result := 0;
end;

constructor TLeaf.Make;
begin
end;

procedure TOver.Go;
begin
end;

procedure TOver.Step;

  procedure Inner;
  begin
    inherited;
  end;

begin
  Inner;
end;

destructor Stray;
begin
end;

var
  B: TBase;
  O: TOther;
  P: TPlain;
  L: TLeaf;
begin
  O := TOther.Create;
  O.Add(1);
  B := O;
  WriteLn(O.UnitName);
  WriteLn(O.QualifiedClassName);
  P := TPlain.Create;
  L := TLeaf.Make;
end.
