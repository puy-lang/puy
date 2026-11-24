program RttiMembersPuy;

// System.Rtti as Puy has it (rtti.md chapters 1 and 5): private and protected members are not
// listed, records are read and written through a TValue holding them (the record given keeps
// its value), an instantiation names its definition and type arguments, a field of the type
// TValue holds the TValue itself, a search by name finds the types of the program only, and the
// errors of reading and writing members

uses System.SysUtils, System.Rtti, System.Generics.Collections;

type
  TKind = (kA, kB, kC);
  TSmall = TKind.kA..TKind.kB;

  IShape = interface
    function Area: Double;
    procedure Scale(Factor: Integer; const Xs: array of Integer);
  end;

  TPoint = record
    X, Y: Integer;
    function Len: Integer;
  end;

  TNode = class(TRefObject)
  public
    Name: string;
  end;

  TBox = class
  private
    FSecret: Integer;
    FCount: Integer;
    function GetCount: Integer;
  protected
    Hidden: Integer;
  public
    Inner: TPoint;
    Tags: TArray<string>;
    Val: TValue;
    Node: TNode;
    [weak] Peer: TNode;
    constructor Create(A: Integer);
    destructor Destroy; override;
    class function Make: TBox; static;
    procedure Generic<T>(const X: T);
    property Count: Integer read FCount write FCount;
    property Computed: Integer read GetCount;
  end;

  TProcT = procedure(A: Integer; const S: string);
  TFuncT = reference to function(X: Double): Boolean;

function TPoint.Len: Integer;
begin
  Result := X + Y;
end;

constructor TBox.Create(A: Integer);
begin
  FSecret := A;
end;

destructor TBox.Destroy;
begin
  inherited;
end;

class function TBox.Make: TBox;
begin
  Result := nil;
end;

procedure TBox.Generic<T>(const X: T);
begin
end;

function TBox.GetCount: Integer;
begin
  Result := FCount * 2;
end;

procedure Show(const What: string; Names: TArray<string>);
begin
  Write(What, ':');
  for var N in Names do
    Write(' ', N);
  WriteLn;
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

// Writes a new node into the field Node and the [weak] field Peer.
procedure SetNodes(B: TBox);
begin
  var N := TNode.Create;
  N.Name := 'node';
  var T := TRttiContext.Create.GetType(TBox);
  T.GetField('Node').SetValue(B, N);
  T.GetField('Peer').SetValue(B, N);
end;

// The name of the node the [weak] field Peer reads, '(gone)' for nil.
function PeerName(B: TBox): string;
begin
  var V := TRttiContext.Create.GetType(TBox).GetField('Peer').GetValue(B);
  if V.IsEmpty then
    Result := '(gone)'
  else
    Result := V.AsType<TNode>.Name;
end;

procedure Main;
var
  Ctx: TRttiContext;
begin
  Ctx := TRttiContext.Create;
  var T := Ctx.GetType(TBox);
  var Names: TArray<string> := nil;
  for var F in T.GetFields do
    Names := Names + [F.Name];
  Show('fields', Names);
  Names := nil;
  for var P in T.GetProperties do
    Names := Names + [P.Name];
  Show('properties', Names);
  Names := nil;
  for var M in T.GetMethods do
    Names := Names + [M.Name];
  Show('methods', Names);
  WriteLn('TObject: ', Length(Ctx.GetType(TObject).GetMethods), ' ',
    Ctx.GetType(TObject).GetMethods[0].Name);
  WriteLn('same objects: ', Ctx.GetType(TypeInfo(TBox)) = T, ' ',
    T.GetField('Inner') = T.GetFields[0], ' ', T.GetField('nOdE').Name);
  WriteLn('no member: ', T.GetField('FSecret') = nil, ' ', T.GetField('Hidden') = nil, ' ',
    T.GetMethod('GetCount') = nil, ' ', T.GetMethod('Generic') = nil);

  for var M in Ctx.GetType(TypeInfo(IShape)).GetMethods do
  begin
    Write(M.Name, '(');
    for var A in M.GetParameters do
    begin
      Write(A.Name, ': ', A.ParamType.Name, ' [');
      for var F := Low(TParamFlag) to High(TParamFlag) do
        if F in A.Flags then
          Write(' ', Ord(F));
      Write(' ] ');
    end;
    WriteLn(')');
  end;
  WriteLn('record method: ', Ctx.GetType(TypeInfo(TPoint)).GetMethods[0].Name);

  // An instantiation
  var L := Ctx.GetType(TypeInfo(TList<Integer>));
  WriteLn(L.Name, ' | ', L.QualifiedName, ' | ', L.GenericDefinitionName, ' | ',
    Length(L.GetGenericArguments), ' ', L.GetGenericArguments[0].QualifiedName);
  WriteLn(L.GetMethod('Add') <> nil, ' ', L.GetField('FItems') = nil);
  WriteLn('not generic: "', T.GenericDefinitionName, '" ', Length(T.GetGenericArguments));

  // The kinds of types
  var E := Ctx.GetType(TypeInfo(TSmall)) as TRttiEnumerationType;
  WriteLn(E.Name, ' ', E.UnderlyingType.Name, ' ', E.MinValue, '..', E.MaxValue, ' ',
    Ord(E.OrdType));
  Show('names', E.GetNames);
  WriteLn(TRttiEnumerationType.GetName<TSmall>(TKind.kB), ' ',
    Ord(TRttiEnumerationType.GetValue<TKind>('KC')));
  var O := Ctx.GetType(TypeInfo(UInt64)).AsOrdinal;
  WriteLn(O.Name, ' ', Ord(O.OrdType), ' ', O.MinValue, ' ', O.MaxValue);
  WriteLn(Ord((Ctx.GetType(TypeInfo(Currency)) as TRttiFloatType).FloatType), ' ',
    (Ctx.GetType(TypeInfo(TArray<string>)) as TRttiDynamicArrayType).ElementType.Name);
  var PT := Ctx.GetType(TypeInfo(TProcT)) as TRttiProcedureType;
  WriteLn(PT.Name, ' ', Length(PT.GetParameters), ' ', PT.GetParameters[1].Name, ' ',
    PT.ReturnType = nil);
  var FT := Ctx.GetType(TypeInfo(TFuncT)) as TRttiProcedureType;
  WriteLn(FT.Name, ' ', FT.ReturnType.Name, ' ', Ord(FT.TypeKind));
  WriteLn((Ctx.GetType(TypeInfo(TClass)) as TRttiClassRefType).InstanceType.Name, ' ',
    (Ctx.GetType(TypeInfo(TBox)) as TRttiInstanceType).MetaclassType.ClassName);

  // By name: the program's types, not those of the standard library it does not reach
  WriteLn(Ctx.FindType('RttiMembersPuy.TPoint').Name, ' ', Ctx.FindType('rttimemberspuy.tbox') = T,
    ' ', Ctx.FindType('System.Integer') <> nil, ' ', Ctx.FindType('System.Classes.TStrings') = nil,
    ' ', Ctx.FindType('TBox') = nil);

  // A record through a TValue: the record given keeps its value
  var R := Default(TPoint);
  R.X := 1;
  R.Y := 2;
  var V := TValue.From<TPoint>(R);
  var W := V;
  var RT := Ctx.GetType(TypeInfo(TPoint));
  RT.GetField('Y').SetValue(V, 40);
  WriteLn(R.Y, ' ', W.AsType<TPoint>.Y, ' ', V.AsType<TPoint>.Y, ' ',
    RT.GetField('X').GetValue(V).ToString);

  // Fields of a class: a flat record, an array, a TValue (not nested), instances
  var B := TBox.Create(5);
  try
    T.GetField('Inner').SetValue(B, V);
    WriteLn(B.Inner.X, ' ', B.Inner.Y, ' ', T.GetField('Inner').GetValue(B).AsType<TPoint>.Len);
    T.GetField('Tags').SetValue(B, TValue.From<TArray<string>>(['a', 'b']));
    WriteLn(Length(B.Tags), B.Tags[1]);
    T.GetField('Val').SetValue(B, 'inner');
    var Got := T.GetField('Val').GetValue(B);
    WriteLn(B.Val.AsString, ' ', Got.AsString, ' ', Got.Kind = TTypeKind.tkUString);
    SetNodes(B);
    WriteLn(B.Node.Name, ' ', PeerName(B));
    B.Node := nil;
    WriteLn('weak after: ', PeerName(B));
    T.GetProperty('Count').SetValue(B, 21);
    WriteLn(B.Count, ' ', T.GetProperty('Count').GetValue(B).AsInteger, ' ',
      T.GetProperty('Computed').IsReadable, ' ', T.GetProperty('Computed').IsWritable);

    Fails('read only', procedure begin T.GetProperty('Computed').SetValue(B, 1); end);
    WriteLn('by a method: ', T.GetProperty('Computed').GetValue(B).ToString);
    Fails('wrong value', procedure begin T.GetField('Tags').SetValue(B, 5); end);
    Fails('wrong instance', procedure begin RT.GetField('X').GetValue(B); end);
    Fails('nil', procedure begin T.GetField('Tags').GetValue(nil); end);
    Fails('empty', procedure begin RT.GetField('X').GetValue(TValue.Empty); end);
    Fails('as record', procedure begin WriteLn(T.AsRecord.Name); end);
    Fails('no value', procedure begin TRttiEnumerationType.GetValue<TKind>('kZ'); end);
  finally
    B.Free;
  end;
  Fails('freed', procedure begin T.GetField('Tags').GetValue(B); end);
end;

begin
  Main;
end.
