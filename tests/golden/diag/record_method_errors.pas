program RecordMethodErrors;

// What a record cannot declare (objects.md chapter 6): inheritance, `abstract` / `sealed`,
// virtual methods, destructors, `protected` / `strict protected` / `published` sections (E0744), a
// class method without `static` (E0744), a constructor without parameters (E0747), methods in a
// record type without a name (E0744); and the uses: an instance member in a static method and
// through the type's name (E0700), a constructor called on a variable (E0747), `inherited`
// (E0700), a `strict private` field from outside (E0743), a property assigned through a receiver
// that cannot be written, whether its writer is a field or a method (E0707).

type
  TBase = record
    X: Integer;
  end;
  TA = record(TBase)
    Y: Integer;
  end;
  TB = record sealed
    Y: Integer;
  end;
  TC = record
    X: Integer;
    procedure V; virtual;
    procedure O; override;
    destructor Destroy;
    constructor Create;
    constructor Make(AX: Integer);
    class procedure K;
    class function S: Integer; static;
    procedure Up;
  protected
    Z: Integer;
  strict protected
    W: Integer;
  published
    Q: Integer;
  end;
  TD = record
  strict private
    FHidden: Integer;
    procedure SetVal(V: Integer);
  public
    property Val: Integer read FHidden write SetVal;
    property Raw: Integer read FHidden write FHidden;
  end;

procedure TC.V;
begin
end;

procedure TC.O;
begin
end;

constructor TC.Create;
begin
end;

constructor TC.Make(AX: Integer);
begin
  X := AX;
end;

class procedure TC.K;
begin
end;

class function TC.S: Integer;
begin
  Result := X;
end;

procedure TC.Up;
begin
  inherited Up;
end;

procedure TD.SetVal(V: Integer);
begin
  FHidden := V;
end;

function MakeD: TD;
begin
  Result := Default(TD);
end;

procedure UseConst(const D: TD);
begin
  D.Val := 1;
  D.Raw := 2;
end;

var
  C: TC;
  D: TD;
  R: record
    procedure M;
  end;
begin
  C := TC.Make(1);
  C.Make(2);
  TC.V;
  WriteLn(TC.X);
  D := MakeD;
  WriteLn(D.FHidden);
  MakeD.Val := 3;
  UseConst(D);
end.
