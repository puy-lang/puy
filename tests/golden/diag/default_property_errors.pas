program DefaultPropertyErrors;

// Errors in the declarations of default array properties (objects.md 12.1-12.3): `default` on a
// property without indices and on a `class property` (E0756), two default properties with the
// same index types (E0701), a property without `default` sharing the name of one with it (E0701,
// at the one without, in either order), default properties of one type with different names
// (E0756), one indexed by `TIndexRange` returning a class managed by hand (E0756, but not
// `TObject` nor a property without `default`), and `default` in a helper (E0753).

type
  TManual = class
  end;

  TFoo = class(TRefObject)
  private
    class var FCount: Integer;
    function GetI(I: Integer): Integer;
    function GetJ(J: Integer): Integer;
    function GetS(const S: string): Integer;
    function GetR(const R: TIndexRange): TManual;
    function GetO(const R: TIndexRange; K: Integer): TObject;
    function GetX: Integer;
    class function GetC(I: Integer): Integer; static;
  public
    property Items[I: Integer]: Integer read GetI; default;
    property Items[J: Integer]: Integer read GetJ; default;
    property Other[const S: string]: Integer read GetS; default;
    property NoIndex: Integer read GetX; default;
    class property Counts[I: Integer]: Integer read GetC; default;
    property Plain[I: Integer]: Integer read GetI;
    property Plain[const S: string]: Integer read GetS; default;
    property Mixed[const S: string]: Integer read GetS; default;
    property Mixed[I: Integer]: Integer read GetI;
    property Items[const R: TIndexRange]: TManual read GetR; default;
    property Items[const R: TIndexRange; K: Integer]: TObject read GetO; default;
    property Slice[const R: TIndexRange]: TManual read GetR;
  end;

  TIntHelper = record helper for Integer
    function Digit(I: Integer): Integer;
    property Digits[I: Integer]: Integer read Digit; default;
  end;

function TFoo.GetI(I: Integer): Integer;
begin
  Result := I;
end;

function TFoo.GetJ(J: Integer): Integer;
begin
  Result := J;
end;

function TFoo.GetS(const S: string): Integer;
begin
  Result := Length(S);
end;

function TFoo.GetR(const R: TIndexRange): TManual;
begin
  Result := nil;
end;

function TFoo.GetO(const R: TIndexRange; K: Integer): TObject;
begin
  Result := nil;
end;

function TFoo.GetX: Integer;
begin
  Result := 0;
end;

class function TFoo.GetC(I: Integer): Integer;
begin
  Result := FCount + I;
end;

function TIntHelper.Digit(I: Integer): Integer;
begin
  Result := I;
end;

begin
end.
