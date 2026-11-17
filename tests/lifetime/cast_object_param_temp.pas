program CastObjectParamTemp;

// Casting a `TObject` value to a class that is not reference counted, then reading a field
// (`TCircle(Obj).Radius`) or passing it (`ShowRadius(TCircle(Obj))`): the cast is checked on a
// counted copy in a temporary of its own, released at the end of the routine, not in the
// uncounted temporary of the field read or the argument slot, where writing it would let go of
// whatever the slot held before as if it were a handle (here the `Boolean` the `is` left, which
// is the handle of the first instance, `Tag`) (docs/design/lifetime.md 3.4).

type
  TCircle = class
  public
    Radius: Integer;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  TTag = class(TRefObject)
  public
    destructor Destroy; override;
  end;

function TCircle.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TCircle) and (TCircle(Obj).Radius = Radius);
end;

function TCircle.GetHashCode: Int64;
begin
  Result := Radius;
end;

destructor TTag.Destroy;
begin
  WriteLn('destroy tag');
  inherited;
end;

procedure ShowRadius(C: TCircle);
begin
  WriteLn('radius ', C.Radius);
end;

procedure ShowObject(Obj: TObject);
begin
  if Obj is TCircle then
    ShowRadius(TCircle(Obj));
end;

var
  Tag: TTag;
  A, B: TCircle;

begin
  Tag := TTag.Create;
  A := TCircle.Create;
  B := TCircle.Create;
  A.Radius := 3;
  B.Radius := 3;
  WriteLn(A.Equals(B), ' ', A.Equals(Tag));
  B.Radius := 4;
  WriteLn(A.Equals(B));
  ShowObject(A);
  ShowObject(Tag);
  ShowObject(B);
  A.Free;
  B.Free;
  WriteLn('done');
end.
