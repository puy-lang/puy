program ValueEqualityRecordBorrow;

// The generated `<eq:…>` / `<hash:…>` of a record with managed fields (equality.md 8.3) take the
// record as `const`: the callers lend it without counting (`.ir`), the fields are read without
// copying the record, and the instance in a field keeps its reference count and is destroyed
// once, when the last reference goes.

type
  TTag = class(TRefObject)
  public
    destructor Destroy; override;
  end;

  TEntry = record
    Name: string;
    Tag: TTag;
  end;

  [ValueEquality]
  THolder = class
  public
    Entry: TEntry;
  end;

destructor TTag.Destroy;
begin
  WriteLn('destroy tag');
  inherited;
end;

var
  A, B: THolder;
  T: TTag;
begin
  A := THolder.Create;
  B := THolder.Create;
  A.Entry.Name := 'x';
  B.Entry.Name := 'x';
  T := TTag.Create;
  A.Entry.Tag := T;
  B.Entry.Tag := T;
  WriteLn('before: ', T.RefCount);
  WriteLn(A.Equals(B), ' ', A.GetHashCode = B.GetHashCode);
  WriteLn('after: ', T.RefCount);
  T := nil;
  A.Free;
  WriteLn('A freed');
  B.Free;
  WriteLn('B freed');
end.
