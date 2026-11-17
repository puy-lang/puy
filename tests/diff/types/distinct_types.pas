program DistinctTypes;

// Distinct types (types.md 1.3) behave as their root type in assignments and operations, and
// an overloaded routine taking the distinct type is chosen for a value of it. (The type of
// `Id + 1` differs from FPC, which keeps the distinct type: golden/runtime/distinct_ops.)

type
  TUserId = type Int64;
  TOrderId = type Int64;
  THtml = type string;
  TMeters = type Double;

procedure Show(Id: TUserId); overload;
begin
  WriteLn('user ', Id);
end;

procedure Show(N: Int64); overload;
begin
  WriteLn('number ', N);
end;

function Twice(Id: TUserId): TUserId;
begin
  Result := Id * 2;
end;

var
  Id: TUserId;
  Order: TOrderId;
  N: Int64;
  I: Integer;
  H: THtml;
  M: TMeters;
begin
  Id := 42;
  N := Id;
  Order := Id;
  I := Id;
  Show(Id);
  Show(N);
  Show(Twice(Id));
  WriteLn(Order, ' ', I, ' ', Id = N, ' ', Order < Id + 1);
  Id := Id + Order;
  WriteLn(Id);
  Id := I;
  WriteLn(Id - 2);
  H := 'abc';
  H := H + '!';
  WriteLn(H, ' ', Length(H), ' ', H[2], ' ', H = 'abc!');
  M := 1.5;
  M := M * 2;
  WriteLn(M:0:2);
end.
