program ClassReleaseFieldsOrder;

// Freeing an instance whose class has no destructor of its own lets go of its fields in their
// order (objects.md 14.8, lifetime.md 3.2): a field freed with it lets go of what it holds before
// the next field is let go of, also when a field is an instance whose destructor runs, a record,
// or an instance without a destructor holding one; a field still held elsewhere stays.

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TPlain = class(TRefObject)
    Inner: TItem;
  end;

  TPair = record
    Item: TItem;
    Tag: string;
  end;

  TOwner = class(TRefObject)
    Name: string;
    A: TItem;
    Count: Integer;
    R: TPair;
    P: TPlain;
    B: TItem;
    C: TItem;
    // Builds the fields here, so that the temporaries holding the new instances are let go when
    // it returns and the fields are their only holders
    constructor Create(const AName: string; Full: Boolean);
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

constructor TOwner.Create(const AName: string; Full: Boolean);
begin
  Name := AName;
  R.Tag := 'tag';
  if not Full then
    Exit;
  A := TItem.Create('a');
  Count := 3;
  R.Item := TItem.Create('r');
  P := TPlain.Create;
  P.Inner := TItem.Create('p-inner');
  B := TItem.Create('b');
  C := TItem.Create('c');
end;

procedure Run;
var
  O: TOwner;
  Keep: TItem;
begin
  O := TOwner.Create('owner', True);
  Keep := O.B;
  O := nil;
  WriteLn('owner cleared');
  WriteLn('keep ', Keep.Name);
  Keep := nil;
  WriteLn('keep cleared');
  // Only strings and plain values: freed without the work list
  O := TOwner.Create('second', False);
  O := nil;
  WriteLn('second cleared');
end;

begin
  Run;
  WriteLn('end');
end.
