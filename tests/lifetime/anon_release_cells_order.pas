program AnonReleaseCellsOrder;

// Letting go of an anonymous method lets go of the cells it captured in their order
// (objects.md 14.9, lifetime.md 3.2): a cell freed with it lets go of its value before the next
// cell is let go of, also when the value is an instance whose destructor runs, and a cell still
// held elsewhere stays. Covers few and many captured variables, which the VM keeps differently.

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
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

function MakeFew: TProc;
var
  A: TItem;
  N: Integer;
  B: TItem;
begin
  A := TItem.Create('few-a');
  N := 1;
  B := TItem.Create('few-b');
  // Two instances among few captures: the cells after the first freed one go through the work
  // list
  Result := procedure
    begin
      WriteLn('few ', A.Name, ' ', N, ' ', B.Name);
    end;
end;

function MakeMany(out Keep: TProc): TProc;
var
  N: Integer;
  S: string;
  A, B, C: TItem;
begin
  N := 2;
  S := 'text';
  A := TItem.Create('many-a');
  B := TItem.Create('many-b');
  C := TItem.Create('many-c');
  Result := procedure
    begin
      WriteLn('many ', N, ' ', S, ' ', A.Name, ' ', B.Name, ' ', C.Name);
    end;
  // Shares the cell of B only
  Keep := procedure
    begin
      WriteLn('keep ', B.Name);
    end;
end;

procedure Run;
var
  F, Keep: TProc;
begin
  F := MakeFew();
  F();
  F := nil;
  WriteLn('few cleared');
  F := MakeMany(Keep);
  F();
  F := nil;
  WriteLn('many cleared');
  Keep();
  Keep := nil;
  WriteLn('keep cleared');
end;

begin
  Run;
  WriteLn('end');
end.
