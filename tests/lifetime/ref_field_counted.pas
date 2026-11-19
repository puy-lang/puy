program RefFieldCounted;

// Managed fields written through a reference (bytecode.md 2.8, `RefFieldSetRef`, and a `TObject`
// field counted by its contents): the old value is let go of when the new one takes its place, a
// record shared with a copy is copied first so that the copy keeps its values, and a variable
// captured by an anonymous method is reached through its cell.

type
  TNote = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;
  TBox = record
    Note: TNote;
    Any: TObject;
    Text: string;
  end;

constructor TNote.Create(const AName: string);
begin
  Name := AName;
end;

destructor TNote.Destroy;
begin
  WriteLn('free ', Name);
  inherited;
end;

procedure Put(var B: TBox; const Name: string);
begin
  B.Note := TNote.Create(Name);
  B.Any := TNote.Create(Name + '-any');
  B.Text := B.Text + Name;
end;

procedure Run;
var
  A, C: TBox;
  P: TProc;
begin
  // A holds the default record: the first write makes its block
  A := Default(TBox);
  Put(A, 'a');
  C := A;
  // A is shared with C: copied first, so C keeps the notes of 'a'
  Put(A, 'b');
  WriteLn(A.Text, ' ', C.Text, ' ', A.Note.Name, ' ', C.Note.Name);
  WriteLn('copy');
  C := A;
  P := procedure
    begin
      A.Text := A.Text + '!';
      A.Note := nil;
      A.Any := nil;
    end;
  P();
  WriteLn(A.Text, ' ', C.Text, ' ', Ord(A.Note = nil), ' ', C.Note.Name);
end;

begin
  Run;
  WriteLn('end');
end.
