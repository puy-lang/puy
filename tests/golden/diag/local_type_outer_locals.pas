// The methods of a type declared in a routine cannot use the routine's local variables,
// parameters, `Result` or nested routines (E0775; types.md 1.1): an instance can outlive the
// routine, and its methods take no static link. Constructors, destructors, the read and write
// methods of properties, class methods, record methods, and routines nested in the methods are
// all checked; the routine's constants and types and routines that are not nested stay usable.
program LocalTypeOuterLocals;

type
  TGetter = reference to function: Integer;

function Top: Integer;
begin
  Result := 1;
end;

function Run(N: Integer): Integer;
const
  Step = 10;
type
  TKind = (kOne, kTwo);
var
  K: Integer;

  procedure Helper;
  begin
    K := K + 1;
  end;

type
  TBox = class
  private
    FValue: Integer;
    function GetValue: Integer;
    procedure SetValue(AValue: Integer);
  public
    constructor Create;
    destructor Destroy; override;
    class function Make: TBox;
    procedure Bump;
    function Fine: Integer;
    property Value: Integer read GetValue write SetValue;
  end;
  TPair = record
    A: Integer;
    procedure Store;
  end;

  constructor TBox.Create;
  begin
    FValue := N;
  end;

  destructor TBox.Destroy;
  begin
    Helper;
    inherited Destroy;
  end;

  class function TBox.Make: TBox;
  begin
    Run := 2;
    Result := TBox.Create;
  end;

  function TBox.GetValue: Integer;
  begin
    Result := FValue + K;
  end;

  procedure TBox.SetValue(AValue: Integer);
  begin
    K := AValue;
    FValue := AValue;
  end;

  procedure TBox.Bump;

    procedure Inner;
    begin
      FValue := K;
    end;

  var
    G: TGetter;
  begin
    Inner;
    G := function: Integer
      begin
        Result := N;
      end;
    // `Result` in a procedure would be that of `Run`
    Result := G();
  end;

  function TBox.Fine: Integer;
  var
    Kind: TKind;
  begin
    Kind := TKind.kTwo;
    Result := Step + Top + Ord(Kind) + Run(0);
  end;

  procedure TPair.Store;
  begin
    A := K;
  end;

begin
  K := N;
  Helper;
  Result := K;
end;

begin
  WriteLn(Run(1));
end.
