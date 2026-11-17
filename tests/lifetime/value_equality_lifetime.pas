program ValueEqualityLifetime;

// Comparing and hashing instances of a `[ValueEquality]` reference counted class (equality.md
// 3.4) does not change their lifetime: the generated `Equals` / `GetHashCode` hold the other
// instance and the fields only while they run, so the reference counts are the same before and
// after, and each instance is destroyed when its last reference goes, nothing left at the end.

type
  TTag = class(TRefObject)
  public
    Name: string;
    destructor Destroy; override;
  end;

  [ValueEquality]
  TItem = class(TRefObject)
  public
    Id: Integer;
    Tag: TTag;
    destructor Destroy; override;
  end;

destructor TTag.Destroy;
begin
  WriteLn('destroy tag ', Name);
  inherited;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy item ', Id);
  inherited;
end;

procedure Run;
var
  A, B: TItem;
  T: TTag;
begin
  T := TTag.Create;
  T.Name := 'shared';
  A := TItem.Create;
  B := TItem.Create;
  A.Id := 1;
  B.Id := 1;
  A.Tag := T;
  B.Tag := T;
  WriteLn('before: ', A.RefCount, ' ', B.RefCount, ' ', T.RefCount);
  WriteLn('equal: ', A.Equals(B), ' ', A.GetHashCode = B.GetHashCode);
  WriteLn('after: ', A.RefCount, ' ', B.RefCount, ' ', T.RefCount);
  T := nil;
  B := nil;
  WriteLn('B released');
  A := nil;
  WriteLn('A released');
end;

begin
  Run;
  WriteLn('done');
end.
