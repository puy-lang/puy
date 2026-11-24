program RttiPTypeInfo;

// PTypeInfo is a value of one slot: assigned, compared with = / <> and nil, held in fields,
// arrays and type arguments, nil by default (rtti.md 2.1). TypeInfo of an alias is its type's,
// of a distinct type its own (2.2), and in a generic body that of each instantiation

type
  TCount = Integer;
  TOther = type Integer;
  TColor = (Red, Green);
  THolder = record
    Info: PTypeInfo;
  end;
  TBox<T> = class
    function Info: PTypeInfo;
  end;

function TBox<T>.Info: PTypeInfo;
begin
  Result := TypeInfo(T);
end;

function Same(A, B: PTypeInfo): Boolean;
begin
  Result := A = B;
end;

var
  P: PTypeInfo;
  H: THolder;
  A: array[0..1] of PTypeInfo;
  L: TArray<PTypeInfo>;
  BI: TBox<Integer>;
  BC: TBox<TColor>;
begin
  WriteLn(P = nil, ' ', Assigned(P));
  P := TypeInfo(Integer);
  WriteLn(P = nil, ' ', Assigned(P));
  WriteLn(Same(TypeInfo(TCount), TypeInfo(Integer)), ' ', Same(TypeInfo(TOther), TypeInfo(Integer)));
  H.Info := TypeInfo(TColor);
  A[0] := H.Info;
  A[1] := Default(PTypeInfo);
  L := [P, A[0]];
  WriteLn(A[0] = TypeInfo(TColor), ' ', A[1] = nil, ' ', L[0] <> L[1]);
  BI := TBox<Integer>.Create;
  BC := TBox<TColor>.Create;
  WriteLn(BI.Info = TypeInfo(Integer), ' ', BC.Info = TypeInfo(TColor), ' ', BI.Info <> BC.Info);
  BI.Free;
  BC.Free;
end.
