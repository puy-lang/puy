program RecordDirectiveErrors;

// Variants of the record diagnostics not covered by golden/diag/record_method_errors (objects.md
// chapter 6): `record abstract`, the virtual directives `dynamic`, `abstract`, `reintroduce` and
// `final` (each "records cannot have virtual methods"), `inherited` alone in a record method, and
// a member a record type does not have.

type
  TA = record abstract
    X: Integer;
  end;

  TB = record
    X: Integer;
    procedure P1; dynamic;
    procedure P2; abstract;
    procedure P3; reintroduce;
    procedure P4; final;
    procedure Up;
  end;

procedure TB.P1;
begin
end;

procedure TB.P2;
begin
end;

procedure TB.P3;
begin
end;

procedure TB.P4;
begin
end;

procedure TB.Up;
begin
  inherited;
end;

begin
  TB.Missing;
end.
