// puy-test: memory-limit=1m
program TValueLifetime;

// A TValue holds the managed value it was made from (rtti.md 3.1, 3.8): an instance of a
// reference counted class or an interface lives while a TValue holds it, an instance managed by
// hand is not held (freeing it is fine), and strings, arrays and records
// made into TValues (and the elements of an `array of const`) are let go of: the loop would run
// out of the 1 MB limit if anything were kept, and nothing is left at the end.

uses System.SysUtils;

type
  TNode = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;
  IThing = interface
    function Name: string;
  end;
  TThing = class(TRefObject, IThing)
    destructor Destroy; override;
    function Name: string;
  end;
  TPlain = class
  end;
  TRec = record
    S: string;
    A: TArray<string>;
  end;

constructor TNode.Create(const AName: string);
begin
  Name := AName;
end;

destructor TNode.Destroy;
begin
  WriteLn('destroy ', Name);
end;

destructor TThing.Destroy;
begin
  WriteLn('destroy thing');
end;

function TThing.Name: string;
begin
  Result := 'thing';
end;

function Count(const Args: array of const): Integer;
begin
  Result := Length(Args);
end;

// The instances held only by TValues and by the temporaries of the routine are let go of when
// it ends (`docs/design/lifetime.md` 3.4)
procedure Hold;
var
  V, W: TValue;
  P: TPlain;
begin
  V := TValue.From<TNode>(TNode.Create('a'));
  W := V;
  V := TValue.Empty;
  WriteLn(V.IsEmpty, ' ', W.IsObject);
  V := TValue.From<TObject>(TNode.Create('b'));
  WriteLn(V.ToString);
  V := TValue.From<IThing>(TThing.Create);
  WriteLn(V.AsType<IThing>.Name);
  P := TPlain.Create;
  V := P;
  P.Free;
  WriteLn(V.IsObject, ' end of Hold');
end;

procedure Churn;
var
  V, W: TValue;
  I: Integer;
  R: TRec;
  Vals: TArray<TValue>;
begin
  V := TValue.Empty;
  R := Default(TRec);
  Vals := nil;
  for I := 1 to 2000 do
  begin
    R.S := StringOfChar('x', 200);
    R.A := [R.S, R.S + 'y'];
    V := TValue.From<TRec>(R);
    W := TValue.From<string>(R.S + IntToStr(I));
    Vals := [V, W, R.S];
    if Count([R.S + 'z', R.A, V, I]) <> 4 then
      WriteLn('wrong count');
  end;
  WriteLn(V.AsType<TRec>.S = R.S, ' ', Length(Vals));
end;

begin
  Hold;
  WriteLn('after Hold');
  Churn;
end.
