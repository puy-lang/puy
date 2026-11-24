program RttiInvokePuy;

// TRttiMethod.Invoke and the attributes as Puy has them (rtti.md 5.7, 5.8): a record's method
// runs on a copy of the record the TValue holds, a record's constructor makes a new record,
// a class without a constructor of its own is made through TObject.Create, a destructor is Free,
// the errors of invoking, and each attribute's instance made once, when first asked for, in the
// order written (a constructor that raises leaves it to be made again). A class method takes
// a class reference only (as Delphi), a constructor refuses an instance, the var arguments are
// written back into a dynamic array only (not a static array, a value), and the methods of an
// instantiated generic type and of a type of the standard library nothing else calls can be
// invoked.

uses System.SysUtils, System.Rtti, System.Generics.Collections;

type
  TTagAttribute = class(TCustomAttribute)
  public
    Tag: string;
    constructor Create(const ATag: string);
  end;

  TOtherAttribute = class(TCustomAttribute)
  end;

  TFlakyAttribute = class(TCustomAttribute)
  public
    constructor Create;
  end;

  TPoint = record
    X, Y: Integer;
    function Sum(Extra: Integer): Integer;
    procedure Move(DX: Integer);
    class function Origin: TPoint; static;
    constructor Create(AX, AY: Integer);
  end;

  IGreeter = interface
    function Greet: string;
  end;

  [TTag('shape'), TOther]
  TShape = class
  public
    [TTag('name')] Name: string;
    [TTag('run'), TOther] procedure Run([TTag('times')] Times: Integer; out Done: Boolean);
    procedure Abstr; virtual; abstract;
    destructor Destroy; override;
    [TTag('size')] property Size: Integer read GetSize;
    function GetSize: Integer;
  end;

  TPlain = class
  end;

  [TFlaky]
  TOdd = class
  end;

  TRc = class(TRefObject)
  end;

  TCounter = class
  public
    class function Kind: string;
    procedure Twice(var N: Integer);
  end;

  TOneValue = array[0..0] of TValue;

  TBox<T> = class
  public
    Value: T;
    function Get: T;
  end;

var
  Made, Flaky: Integer;

constructor TTagAttribute.Create(const ATag: string);
begin
  Inc(Made);
  Tag := ATag;
end;

constructor TFlakyAttribute.Create;
begin
  Inc(Flaky);
  if Flaky = 1 then
    raise EConvertError.Create('first try');
end;

function TPoint.Sum(Extra: Integer): Integer;
begin
  Result := X + Y + Extra;
end;

procedure TPoint.Move(DX: Integer);
begin
  X := X + DX;
end;

class function TPoint.Origin: TPoint;
begin
  Result := Default(TPoint);
  Result.Y := 4;
end;

constructor TPoint.Create(AX, AY: Integer);
begin
  X := AX;
  Y := AY;
end;

procedure TShape.Run(Times: Integer; out Done: Boolean);
begin
  Done := Times > 2;
end;

destructor TShape.Destroy;
begin
  inherited;
end;

function TShape.GetSize: Integer;
begin
  Result := 9;
end;

class function TCounter.Kind: string;
begin
  Result := 'counter';
end;

procedure TCounter.Twice(var N: Integer);
begin
  N := N * 2;
end;

function TBox<T>.Get: T;
begin
  Result := Value;
end;

procedure Fails(const What: string; const P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

function Names(const A: TArray<TCustomAttribute>): string;
begin
  Result := '';
  for var X in A do
    if X is TTagAttribute then
      Result := Result + ' ' + TTagAttribute(X).Tag
    else
      Result := Result + ' ' + X.ClassName;
end;

var
  Ctx: TRttiContext;
  T, PT: TRttiType;
  P: TPoint;
  V: TValue;
  A: TArray<TValue>;
  S: TShape;
  C: TClass;
begin
  Ctx := TRttiContext.Create;
  T := Ctx.GetType(TypeInfo(TShape));

  // The attributes, made once each
  WriteLn('made before: ', Made);
  WriteLn('type:', Names(T.GetAttributes), ' made ', Made);
  WriteLn('same: ', T.GetAttributes[0] = T.GetAttributes[0], ' made ', Made);
  WriteLn('field:', Names(T.GetField('Name').GetAttributes));
  WriteLn('method:', Names(T.GetMethod('Run').GetAttributes));
  WriteLn('parameter:', Names(T.GetMethod('Run').GetParameters[0].GetAttributes));
  WriteLn('property:', Names(T.GetProperty('Size').GetAttributes));
  WriteLn('none:', Names(T.GetMethod('Abstr').GetAttributes), ' ',
    Length(Ctx.GetType(TPlain).GetAttributes));
  WriteLn('get: ', T.GetField('Name').GetAttribute<TTagAttribute>.Tag, ' ',
    T.GetField('Name').GetAttribute<TOtherAttribute> = nil, ' ',
    T.HasAttribute<TOtherAttribute>, ' ', T.GetField('Name').HasAttribute<TOtherAttribute>);
  Fails('flaky', procedure begin Ctx.GetType(TOdd).GetAttributes; end);
  WriteLn('flaky again: ', Names(Ctx.GetType(TOdd).GetAttributes), ' ', Flaky);

  // A record's methods: Self is a copy
  PT := Ctx.GetType(TypeInfo(TPoint));
  P.X := 1;
  P.Y := 2;
  V := TValue.From<TPoint>(P);
  WriteLn('sum: ', PT.GetMethod('Sum').Invoke(V, [10]).AsInteger);
  PT.GetMethod('Move').Invoke(V, [5]);
  WriteLn('moved copy: ', V.AsType<TPoint>.X);
  WriteLn('static: ', PT.GetMethod('Origin').Invoke(V, []).AsType<TPoint>.Y);
  WriteLn('record constructor: ', PT.GetMethod('Create').Invoke(V, [7, 8]).AsType<TPoint>.Y);

  // A class with no constructor of its own, a destructor
  C := TPlain;
  V := Ctx.GetType(TPlain).GetMethod('Create').Invoke(C, []);
  WriteLn('made: ', V.AsObject.ClassName);
  Ctx.GetType(TPlain).GetMethod('Destroy').Invoke(V.AsObject, []);

  // The errors
  C := TShape;
  S := TShape(Ctx.GetType(TShape).GetMethod('Create').Invoke(C, []).AsObject);
  SetLength(A, 2);
  A[0] := 1;
  Fails('count', procedure begin T.GetMethod('Run').Invoke(S, [1]); end);
  Fails('type', procedure begin T.GetMethod('Run').Invoke(S, ['x', False]); end);
  Fails('abstract', procedure begin T.GetMethod('Abstr').Invoke(S, []); end);
  Fails('on instance', procedure begin T.GetMethod('Create').Invoke(S, []); end);
  Fails('nil', procedure
    var N: TObject;
    begin
      N := nil;
      T.GetMethod('Run').Invoke(N, A);
    end);
  Fails('other class', procedure
    var O: TPlain;
    begin
      O := TPlain.Create;
      try
        T.GetMethod('Run').Invoke(O, A);
      finally
        O.Free;
      end;
    end);
  Fails('record type', procedure begin PT.GetMethod('Sum').Invoke(S, [1]); end);
  Fails('empty record', procedure begin PT.GetMethod('Sum').Invoke(TValue.Empty, [1]); end);
  Fails('interface',
    procedure begin Ctx.GetType(TypeInfo(IGreeter)).GetMethod('Greet').Invoke(S, []); end);
  Fails('reference counted', procedure
    begin
      Ctx.GetType(TRc).GetMethod('Destroy').Invoke(TRc.Create, []);
    end);
  Fails('destroy other class', procedure
    var O: TPlain;
    begin
      O := TPlain.Create;
      try
        T.GetMethod('Destroy').Invoke(O, []);
      finally
        O.Free;
      end;
    end);
  Fails('destroy nil', procedure
    var N: TObject;
    begin
      N := nil;
      T.GetMethod('Destroy').Invoke(N, []);
    end);
  S.Free;
  Fails('freed', procedure begin T.GetMethod('Run').Invoke(S, A); end);

  // A class method takes a class reference, not an instance
  var Counter := TCounter.Create;
  var CT := Ctx.GetType(TCounter);
  WriteLn('class method: ', CT.GetMethod('Kind').Invoke(TCounter, []).AsString);
  Fails('class method on instance', procedure begin CT.GetMethod('Kind').Invoke(Counter, []); end);

  // The var arguments: written back into a dynamic array, not into a static array nor `[…]`
  var Dyn: TArray<TValue>;
  SetLength(Dyn, 1);
  Dyn[0] := 3;
  var Stat := Default(TOneValue);
  Stat[0] := 3;
  CT.GetMethod('Twice').Invoke(Counter, Dyn);
  CT.GetMethod('Twice').Invoke(Counter, Stat);
  CT.GetMethod('Twice').Invoke(Counter, [3]);
  WriteLn('written back: ', Dyn[0].AsInteger, ' ', Stat[0].AsInteger);
  Counter.Free;

  // An instantiated generic type, a type of the standard library
  var Box := TBox<Integer>.Create;
  Box.Value := 7;
  WriteLn('generic: ',
    Ctx.GetType(TypeInfo(TBox<Integer>)).GetMethod('Get').Invoke(Box, []).AsInteger);
  Box.Free;
  var L := TList<Integer>.Create;
  Ctx.GetType(TypeInfo(TList<Integer>)).GetMethod('Add').Invoke(L, [5]);
  WriteLn('list: ', L.Count, ' ', L[0]);
  L.Free;
  var Fields := Ctx.GetType(TRttiType).GetMethod('GetDeclaredFields').Invoke(T, []);
  WriteLn('standard library: ', Fields.GetArrayLength);
end.
