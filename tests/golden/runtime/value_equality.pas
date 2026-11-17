program ValueEquality;

// `[ValueEquality]` and `[EqualityMember]` (equality.md 3.4): the generated `Equals` compares
// the fields marked `[EqualityMember]` (or every field when none is marked) by key equality, and
// only instances of the same class are equal; the generated `GetHashCode` gives equal instances
// the same hash. Descendants are treated as `[ValueEquality]` and compare their own fields too,
// until one overrides both `Equals` and `GetHashCode`. Fields of every kind are compared by
// their key rules (strings by content, `0.0` = `-0.0`, NaN = NaN, records and dynamic arrays by
// their contents, classes through `Equals` without touching a destroyed instance, routines and
// method references by the `=` rule), and fields linking to instances of the class are followed.
// Each instantiation of a generic class gets its own, comparing the fields of its type arguments.

type
  TPoint = record
    X, Y: Integer;
    Name: string;
  end;

  TColor = (Red, Green, Blue);
  TLevels = array[TColor] of Integer;

  // Only the marked fields are compared.
  [ValueEquality]
  TMoney = class(TRefObject)
  public
    [EqualityMember] Amount: Currency;
    [EqualityMember] Code: string;
    Note: string;
  end;

  // A child of a class choosing fields: its new field is compared only when marked.
  TTaggedMoney = class(TMoney)
  public
    Tag: string;
    [EqualityMember] Region: string;
  end;

  // Every field is compared.
  [ValueEquality]
  TShape = class
  public
    Kind: string;
    Size: Double;
  end;

  // Inherits `[ValueEquality]`: its own field is compared too.
  TCircle = class(TShape)
  public
    Radius: Integer;
  end;

  // Overrides both: the generation stops here.
  TLooseCircle = class(TCircle)
  public
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  // Inherits the user's `Equals` / `GetHashCode` of `TLooseCircle`.
  TLooseRing = class(TLooseCircle)
  public
    Inner: Integer;
  end;

  // Writing the attribute again generates from here.
  [ValueEquality]
  TStrictRing = class(TLooseCircle)
  public
    Inner: Integer;
  end;

  TNotify = procedure(N: Integer);
  TOnEvent = procedure(N: Integer) of object;

  TListener = class
  public
    procedure Handle(N: Integer);
  end;

  // Fields of the other kinds.
  [ValueEquality]
  TBag = class
  public
    Point: TPoint;
    Items: array of TPoint;
    Levels: TLevels;
    Flags: set of TColor;
    Owner: TObject;
    Proc: TNotify;
    Event: TOnEvent;
    Text: UTF8String;
    Ratio: Single;
  end;

  // Linked instances are compared by following the links.
  [ValueEquality]
  TNode = class
  public
    Value: Integer;
    Next: TNode;
  end;

  IShape = interface
    function Area: Integer;
  end;

  TSquare = class(TRefObject, IShape)
  public
    Side: Integer;
    function Area: Integer;
  end;

  // Each instantiation compares the fields of its type arguments.
  [ValueEquality]
  TPair<K, V> = class(TRefObject)
  public
    Key: K;
    Value: V;
    Shape: IShape;
  end;

  // An exception class may have it too.
  [ValueEquality]
  EKeyed = class(Exception)
  public
    Key: Integer;
  end;

function KeyHashOf(N: Integer): Int64;
begin
  Result := HashCombine(0, N);
end;

function TLooseCircle.Equals(Obj: TObject): Boolean;
var
  C: TCircle;
begin
  // Any circle of the same radius
  Result := False;
  if Obj is TCircle then
  begin
    C := TCircle(Obj);
    Result := C.Radius = Radius;
  end;
end;

function TLooseCircle.GetHashCode: Int64;
begin
  Result := KeyHashOf(Radius);
end;

function TSquare.Area: Integer;
begin
  Result := Side * Side;
end;

procedure TListener.Handle(N: Integer);
begin
end;

procedure Notify1(N: Integer);
begin
end;

procedure Notify2(N: Integer);
begin
end;

procedure Show(const What: string; A, B: TObject);
begin
  WriteLn(What, ': ', A.Equals(B), ' ', B.Equals(A), ' ', A.Equals(B) <= (A.GetHashCode = B.GetHashCode));
end;

var
  M1, M2: TMoney;
  T1, T2: TTaggedMoney;
  S1, S2: TShape;
  C1, C2: TCircle;
  L1: TLooseCircle;
  R1, R2: TLooseRing;
  X1, X2: TStrictRing;
  B1, B2: TBag;
  N1, N2, N3, N4: TNode;
  E1, E2: EKeyed;
  P1, P2: TPair<string, TPoint>;
  Q1, Q2: TPair<Integer, TArray<Double>>;
  Sq: TSquare;
  Listener, Other: TListener;
  Zero, NaN: Double;
begin
  M1 := TMoney.Create;
  M2 := TMoney.Create;
  M1.Amount := 1.5;
  M1.Code := 'JPY';
  M1.Note := 'first';
  M2.Amount := 1.5;
  M2.Code := 'J' + 'PY';
  M2.Note := 'second';
  Show('marked fields', M1, M2);
  M2.Code := 'USD';
  Show('marked field differs', M1, M2);
  WriteLn('nil: ', M1.Equals(nil));
  Show('self', M1, M1);

  T1 := TTaggedMoney.Create;
  T2 := TTaggedMoney.Create;
  T1.Amount := 2;
  T2.Amount := 2;
  T1.Tag := 'a';
  T2.Tag := 'b';
  Show('child, unmarked field differs', T1, T2);
  T2.Region := 'EU';
  Show('child, marked field differs', T1, T2);
  M2.Code := '';
  M2.Amount := 2;
  Show('parent and child', M2, T1);

  S1 := TShape.Create;
  S2 := TShape.Create;
  S1.Kind := 'sq';
  S2.Kind := 'sq';
  Zero := 0.0;
  S1.Size := Zero;
  S2.Size := -Zero;
  Show('0.0 and -0.0', S1, S2);
  NaN := Zero / Zero;
  S1.Size := NaN;
  S2.Size := NaN;
  Show('NaN and NaN', S1, S2);
  S2.Size := 1;
  Show('NaN and 1', S1, S2);

  C1 := TCircle.Create;
  C2 := TCircle.Create;
  C1.Radius := 3;
  C2.Radius := 3;
  Show('inherited', C1, C2);
  C2.Radius := 4;
  Show('inherited, own field differs', C1, C2);
  Show('shape and circle', S1, C1);

  L1 := TLooseCircle.Create;
  L1.Radius := 3;
  L1.Kind := 'other';
  WriteLn('user Equals: ', L1.Equals(C1));
  R1 := TLooseRing.Create;
  R2 := TLooseRing.Create;
  R1.Radius := 5;
  R2.Radius := 5;
  R1.Inner := 1;
  R2.Inner := 2;
  Show('below the user override', R1, R2);
  X1 := TStrictRing.Create;
  X2 := TStrictRing.Create;
  X1.Radius := 5;
  X2.Radius := 5;
  X1.Inner := 1;
  X2.Inner := 2;
  Show('written again', X1, X2);
  X2.Inner := 1;
  Show('written again, same', X1, X2);

  Listener := TListener.Create;
  Other := TListener.Create;
  B1 := TBag.Create;
  B2 := TBag.Create;
  B1.Point.Name := 'p';
  B2.Point.Name := 'p';
  SetLength(B1.Items, 2);
  SetLength(B2.Items, 2);
  B1.Items[1].X := 7;
  B2.Items[1].X := 7;
  B1.Levels[TColor.Blue] := 3;
  B2.Levels[TColor.Blue] := 3;
  B1.Flags := [TColor.Red, TColor.Blue];
  B2.Flags := [TColor.Blue, TColor.Red];
  B1.Owner := M1;
  B2.Owner := M1;
  B1.Proc := Notify1;
  B2.Proc := Notify1;
  B1.Event := Listener.Handle;
  B2.Event := Listener.Handle;
  B1.Text := 'ü';
  B2.Text := 'ü';
  B1.Ratio := 0.5;
  B2.Ratio := 0.5;
  Show('every kind', B1, B2);
  B2.Items[0].Name := 'x';
  Show('array element differs', B1, B2);
  B2.Items[0].Name := '';
  SetLength(B2.Items, 3);
  Show('array length differs', B1, B2);
  SetLength(B2.Items, 2);
  B2.Levels[TColor.Red] := 1;
  Show('static array element differs', B1, B2);
  B2.Levels[TColor.Red] := 0;
  M2.Amount := 1.5;
  M2.Code := 'JPY';
  B2.Owner := M2;
  Show('owners equal by Equals', B1, B2);
  M2.Code := 'other';
  Show('owners not equal', B1, B2);
  B2.Owner := M1;
  B2.Proc := Notify2;
  Show('routine differs', B1, B2);
  B2.Proc := Notify1;
  B2.Event := Other.Handle;
  Show('method reference differs', B1, B2);
  B2.Event := Listener.Handle;
  Show('back to equal', B1, B2);

  // A destroyed instance in a field: compared and hashed by its handle, without an exception
  B1.Owner := Other;
  B2.Owner := Other;
  B1.Event := nil;
  B2.Event := nil;
  Other.Free;
  Show('destroyed owner, same handle', B1, B2);
  B2.Owner := Listener;
  Show('destroyed owner and a live one', B1, B2);

  N1 := TNode.Create;
  N2 := TNode.Create;
  N3 := TNode.Create;
  N4 := TNode.Create;
  N1.Value := 1;
  N1.Next := N2;
  N2.Value := 2;
  N3.Value := 1;
  N3.Next := N4;
  N4.Value := 2;
  Show('linked', N1, N3);
  N4.Value := 3;
  Show('linked, tail differs', N1, N3);

  P1 := TPair<string, TPoint>.Create;
  P2 := TPair<string, TPoint>.Create;
  P1.Key := 'k';
  P2.Key := 'k';
  P1.Value.Name := 'v';
  P2.Value.Name := 'v';
  Sq := TSquare.Create;
  P1.Shape := Sq;
  P2.Shape := Sq;
  Show('instantiation', P1, P2);
  P2.Value.Y := 1;
  Show('instantiation, record field differs', P1, P2);
  P2.Value.Y := 0;
  P2.Shape := TSquare.Create;
  Show('instantiation, interface field differs', P1, P2);
  Q1 := TPair<Integer, TArray<Double>>.Create;
  Q2 := TPair<Integer, TArray<Double>>.Create;
  Q1.Value := [1.5, Zero];
  Q2.Value := [1.5, -Zero];
  Show('another instantiation', Q1, Q2);
  Show('different instantiations', P1, Q1);

  E1 := EKeyed.Create('a');
  E2 := EKeyed.Create('a');
  E1.Key := 1;
  E2.Key := 1;
  Show('exception', E1, E2);
  E2.Message := 'b';
  Show('exception, message differs', E1, E2);

  S1.Free;
  S2.Free;
  C1.Free;
  C2.Free;
  L1.Free;
  R1.Free;
  R2.Free;
  X1.Free;
  X2.Free;
  B1.Free;
  B2.Free;
  N1.Free;
  N2.Free;
  N3.Free;
  N4.Free;
  Listener.Free;
end.
