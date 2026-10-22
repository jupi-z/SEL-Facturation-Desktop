"""PDF vectoriel A4 sélectionnable (ReportLab) pour les décomptes SEL."""
from __future__ import annotations
from io import BytesIO
import json
from decimal import Decimal
from pathlib import Path
from xml.sax.saxutils import escape
import sys

from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_RIGHT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.graphics.barcode.qr import QrCodeWidget
from reportlab.graphics.shapes import Drawing
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, Image
from domain import fmt

# Palette officielle SEL : bleu institutionnel dominant, bleu d'accent et jaune du logo.
BLUE=colors.HexColor('#003078')
ACCENT=colors.HexColor('#1080F8')
YELLOW=colors.HexColor('#F8D800')
PALE=colors.HexColor('#EAF4FF')
INK=colors.HexColor('#142B43')
GREY=colors.HexColor('#5C7185')
BORDER=colors.HexColor('#D7E2EC')


def p(value):
    return escape(str(value if value not in (None,'') else 'Non renseigné'))


def rate_fmt(value):
    return str(value).replace('.', ',') if value not in (None, '') else '—'


def period_fr(period):
    names=['janvier','février','mars','avril','mai','juin','juillet','août','septembre','octobre','novembre','décembre']
    try:
        year,month=period.split('-');return names[int(month)-1].capitalize()+' '+year
    except (ValueError,IndexError):return period


def internal_reference_payload(invoice) -> str:
    """Return the stable, local-only QR payload for a combined invoice."""
    inv = dict(invoice)
    snapshot = json.loads(inv['calc_snapshot'])
    if snapshot.get('document_kind') != 'COMBINED_WATER_ELECTRICITY':
        raise ValueError('Le QR interne est réservé aux documents combinés.')
    payload = {
        'kind': 'SEL_INTERNAL_REFERENCE',
        'v': 1,
        'issuer': 'SEL',
        'number': str(inv['number']),
        'issued': str(inv['issue_date']),
        'currency': str(snapshot.get('currency') or inv.get('currency') or ''),
        'total': str(snapshot['total']),
    }
    return json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(',', ':'))


def _qr_drawing(payload: str):
    widget = QrCodeWidget(payload)
    size = 32 * mm
    drawing = Drawing(size, size)
    drawing.add(widget)
    return drawing


def _render_combined_invoice(inv, payments, client, company, snapshot) -> bytes:
    paid = sum((Decimal(str(x['amount'])) for x in payments), Decimal('0'))
    balance = Decimal(snapshot['total']) - paid
    out = BytesIO()
    doc = SimpleDocTemplate(
        out, pagesize=A4, rightMargin=15 * mm, leftMargin=15 * mm,
        topMargin=15 * mm, bottomMargin=17 * mm,
        title=f"Décompte combiné {inv['number']} - SEL", author='SEL Facturation',
    )
    normal = ParagraphStyle('combined_normal', fontName='Helvetica', fontSize=8.4, leading=11.5, textColor=INK)
    small = ParagraphStyle('combined_small', parent=normal, fontSize=7.3, leading=9.5, textColor=GREY)
    section = ParagraphStyle('combined_section', fontName='Helvetica-Bold', fontSize=10, leading=13, textColor=BLUE, spaceBefore=7, spaceAfter=4)
    title = ParagraphStyle('combined_title', fontName='Helvetica-Bold', fontSize=14, leading=17, textColor=BLUE, alignment=TA_RIGHT)
    head = ParagraphStyle('combined_head', fontName='Helvetica-Bold', fontSize=8, leading=10, textColor=colors.white)
    right = ParagraphStyle('combined_right', parent=normal, alignment=TA_RIGHT)
    total_label = ParagraphStyle('combined_total_label', parent=normal, fontName='Helvetica-Bold', textColor=BLUE)
    total_value = ParagraphStyle('combined_total_value', parent=right, fontName='Helvetica-Bold', fontSize=11, textColor=BLUE)
    story = []

    logo = Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parent)) / 'static' / 'sel-icon.png'
    mark = Image(str(logo), width=18 * mm, height=18 * mm, hAlign='LEFT') if logo.exists() else Paragraph('<b>SEL</b>', title)
    company_cells = [
        mark,
        [Paragraph('<b>' + p(company.get('company_name', 'SOCIÉTÉ D’ÉNERGIE DU LUALABA')) + '</b>', ParagraphStyle('combined_company', parent=normal, fontName='Helvetica-Bold', fontSize=9.5, textColor=BLUE)),
         Paragraph(p(company.get('address')), small),
         Paragraph(' · '.join([p(v) for v in (company.get('phone'), company.get('email'), company.get('website')) if v]), small)],
        [Paragraph('DÉCOMPTE DE CONSOMMATION — EAU &amp; ÉLECTRICITÉ', title),
         Paragraph('N° ' + p(inv['number']), ParagraphStyle('combined_ref', parent=normal, fontName='Helvetica-Bold', alignment=TA_RIGHT, textColor=BLUE)),
         Paragraph('Émis le ' + p(inv['issue_date']), right)],
    ]
    header = Table([company_cells], colWidths=[23 * mm, 96 * mm, 59 * mm])
    header.setStyle(TableStyle([
        ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'), ('LEFTPADDING', (0, 0), (-1, -1), 0),
        ('RIGHTPADDING', (0, 0), (-1, -1), 3), ('TOPPADDING', (0, 0), (-1, -1), 1),
        ('BOTTOMPADDING', (0, 0), (-1, -1), 6), ('LINEBELOW', (0, 0), (-1, -1), 1.6, BLUE),
    ]))
    story.extend([header, Spacer(1, 5)])

    info = Table([[
        Paragraph(
            '<b>CLIENT / CONTRAT</b><br/>' + '<br/>'.join([
                f"<b>Nom :</b> {p(client.get('name'))}", f"<b>Code :</b> {p(client.get('code'))}",
                f"<b>Contrat :</b> {p(client.get('contract_number'))}", f"<b>Adresse :</b> {p(client.get('address'))}",
                f"<b>Type :</b> {p(client.get('category'))}", f"<b>NIF client :</b> {p(client.get('nif'))}",
            ])
        ),
        Paragraph(
            '<b>PÉRIODE ET SERVICE</b><br/>' + '<br/>'.join([
                f"<b>Centre :</b> {p(client.get('center'))}", f"<b>Point de service :</b> {p(client.get('service_point'))}",
                f"<b>Période :</b> {p(period_fr(inv['billing_period']))}", f"<b>Échéance :</b> {p(inv.get('due_date'))}",
                '<b>Document :</b> Eau + électricité',
            ])
        ),
    ]], colWidths=[89 * mm, 89 * mm])
    info.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, -1), colors.HexColor('#F5F9FB')), ('BOX', (0, 0), (-1, -1), .6, BORDER),
        ('INNERGRID', (0, 0), (-1, -1), .4, BORDER), ('VALIGN', (0, 0), (-1, -1), 'TOP'),
        ('TOPPADDING', (0, 0), (-1, -1), 8), ('BOTTOMPADDING', (0, 0), (-1, -1), 8),
        ('LEFTPADDING', (0, 0), (-1, -1), 8),
    ]))
    story.extend([info, Paragraph('SERVICES FACTURÉS', section)])

    service_summaries = []
    for service in snapshot['services']:
        code = service['code']
        label = 'ÉLECTRICITÉ' if code == 'ELECTRICITY' else 'EAU'
        unit = p(service['unit'])
        rows = [[Paragraph('Description', head), Paragraph('Quantité', head), Paragraph('Prix unitaire HT', head), Paragraph('Montant HT', head)]]
        for line in service['lines']:
            rows.append([Paragraph(p(line['label']), normal), Paragraph(p(line['quantity']) + ' ' + unit, right), Paragraph(rate_fmt(line['unit_price']) + ' ' + p(snapshot['currency']), right), Paragraph(fmt(line['amount']), right)])
        table = Table(rows, colWidths=[72 * mm, 31 * mm, 38 * mm, 37 * mm], repeatRows=1)
        table.setStyle(TableStyle([
            ('BACKGROUND', (0, 0), (-1, 0), BLUE), ('ROWBACKGROUNDS', (0, 1), (-1, -1), [colors.white, colors.HexColor('#F7FAFC')]),
            ('INNERGRID', (0, 0), (-1, -1), .4, BORDER), ('BOX', (0, 0), (-1, -1), .6, BORDER),
            ('VALIGN', (0, 0), (-1, -1), 'MIDDLE'), ('LEFTPADDING', (0, 0), (-1, -1), 6),
            ('RIGHTPADDING', (0, 0), (-1, -1), 6), ('TOPPADDING', (0, 0), (-1, -1), 5),
            ('BOTTOMPADDING', (0, 0), (-1, -1), 5),
        ]))
        taxes = sum((Decimal(tax['amount']) for tax in service.get('taxes', [])), Decimal('0'))
        service_summaries.append((label, service['subtotal_excl_tax'], taxes, service['total_incl_tax']))
        story.extend([Paragraph(label, section), table])

    recap_rows = []
    for label, subtotal, taxes, total in service_summaries:
        recap_rows.extend([
            (f'Sous-total {label.lower()}', fmt(subtotal) + ' ' + p(snapshot['currency'])),
            (f'Taxes {label.lower()}', fmt(taxes) + ' ' + p(snapshot['currency'])),
            (f'Total {label.lower()}', fmt(total) + ' ' + p(snapshot['currency'])),
        ])
    if snapshot.get('conversion_rate'):
        recap_rows.append(('Conversion de présentation', f"1 {p(snapshot.get('conversion_source_currency'))} = {rate_fmt(snapshot['conversion_rate'])} {p(snapshot['currency'])}"))
    recap_rows.extend([
        ('SOUS-TOTAL GÉNÉRAL', fmt(snapshot['subtotal_excl_tax']) + ' ' + p(snapshot['currency'])),
        ('TAXES TOTALES', fmt(snapshot['tax_total']) + ' ' + p(snapshot['currency'])),
        ('MONTANT TOTAL (' + p(snapshot['currency']) + ')', fmt(snapshot['total'])),
    ])
    recap = Table([[Paragraph(label, total_label if label.startswith('MONTANT') else normal), Paragraph(value, total_value if label.startswith('MONTANT') else right)] for label, value in recap_rows], colWidths=[130 * mm, 48 * mm])
    recap.setStyle(TableStyle([
        ('LINEABOVE', (0, -1), (-1, -1), 1.2, BLUE), ('BACKGROUND', (0, -1), (-1, -1), PALE),
        ('TOPPADDING', (0, 0), (-1, -1), 4), ('BOTTOMPADDING', (0, 0), (-1, -1), 4),
        ('LEFTPADDING', (0, 0), (0, -1), 7), ('RIGHTPADDING', (1, 0), (1, -1), 7),
    ]))
    story.extend([Spacer(1, 6), recap, Spacer(1, 7)])

    qr = _qr_drawing(internal_reference_payload(inv))
    payment = Table([
        [Paragraph('CONDITIONS DE RÈGLEMENT', head), Paragraph('RÉFÉRENCE INTERNE SEL', head)],
        [Paragraph(f"Échéance : <b>{p(inv.get('due_date'))}</b><br/>Paiement accepté selon les modalités de SEL.<br/><br/>Déjà réglé : <b>{fmt(paid)} {p(snapshot['currency'])}</b><br/>MONTANT À RÉGLER : <b>{fmt(balance)} {p(snapshot['currency'])}</b>", normal),
         [qr, Paragraph('Référence interne SEL', ParagraphStyle('combined_qr_label', parent=small, alignment=TA_CENTER, textColor=BLUE))]],
    ], colWidths=[118 * mm, 60 * mm])
    payment.setStyle(TableStyle([
        ('BACKGROUND', (0, 0), (-1, 0), BLUE), ('BACKGROUND', (0, 1), (-1, 1), colors.HexColor('#F5F9FB')),
        ('BOX', (0, 0), (-1, -1), .65, BORDER), ('INNERGRID', (0, 0), (-1, -1), .45, BORDER),
        ('VALIGN', (0, 0), (-1, -1), 'TOP'), ('ALIGN', (1, 1), (1, 1), 'CENTER'),
        ('TOPPADDING', (0, 0), (-1, -1), 7), ('BOTTOMPADDING', (0, 0), (-1, -1), 7),
        ('LEFTPADDING', (0, 0), (-1, -1), 8), ('RIGHTPADDING', (0, 0), (-1, -1), 8),
    ]))
    story.append(payment)
    if inv.get('status') == 'VOID':
        story.append(Paragraph('<font color="red"><b>DOCUMENT ANNULÉ - NE PAS UTILISER</b></font>', section))
    if inv.get('note'):
        story.extend([Spacer(1, 5), Paragraph('<b>Note :</b> ' + p(inv['note']), small)])

    def footer(canvas, document):
        canvas.saveState()
        width, _ = A4
        canvas.setStrokeColor(BORDER)
        canvas.line(15 * mm, 14 * mm, width - 15 * mm, 14 * mm)
        canvas.setFont('Helvetica', 7)
        canvas.setFillColor(GREY)
        canvas.drawString(15 * mm, 10 * mm, 'SEL - Service clientèle  |  ' + str(inv['number']))
        canvas.drawRightString(width - 15 * mm, 10 * mm, f'Page {document.page}')
        canvas.restoreState()

    doc.build(story, onFirstPage=footer, onLaterPages=footer)
    return out.getvalue()


def render_invoice(invoice, payments) -> bytes:
    inv=dict(invoice)
    client=json.loads(inv['client_snapshot']);co=json.loads(inv['company_snapshot']);d=json.loads(inv['calc_snapshot'])
    if d.get('document_kind') == 'COMBINED_WATER_ELECTRICITY':
        return _render_combined_invoice(inv, payments, client, co, d)
    paid=sum((Decimal(str(x['amount'])) for x in payments),Decimal('0'))
    balance=Decimal(d['total'])-paid
    out=BytesIO()
    doc=SimpleDocTemplate(out,pagesize=A4,rightMargin=15*mm,leftMargin=15*mm,topMargin=16*mm,bottomMargin=17*mm,
                          title=f"Décompte {inv['number']} - SEL",author='SEL Facturation')
    normal=ParagraphStyle('normal',fontName='Helvetica',fontSize=8.6,leading=13,textColor=INK,spaceAfter=0)
    muted=ParagraphStyle('muted',parent=normal,fontSize=8,textColor=GREY)
    small=ParagraphStyle('small',parent=normal,fontSize=7.5,leading=11)
    section=ParagraphStyle('section',fontName='Helvetica-Bold',fontSize=10.3,leading=14,textColor=BLUE,spaceBefore=9,spaceAfter=5)
    title=ParagraphStyle('title',fontName='Helvetica-Bold',fontSize=17,leading=22,textColor=BLUE,spaceAfter=4)
    document_title=ParagraphStyle('document_title',parent=title,fontSize=10.8,leading=13,alignment=TA_RIGHT,spaceAfter=3)
    twhite=ParagraphStyle('twhite',fontName='Helvetica-Bold',fontSize=8.2,leading=11,textColor=colors.white)
    right=ParagraphStyle('right',parent=normal,alignment=TA_RIGHT)
    val=ParagraphStyle('val',parent=normal,alignment=TA_RIGHT,fontSize=9)
    total_label=ParagraphStyle('total_label',parent=normal,fontName='Helvetica-Bold',fontSize=9.5,textColor=BLUE)
    total_value=ParagraphStyle('total_value',parent=right,fontName='Helvetica-Bold',fontSize=11,textColor=BLUE)
    story=[]

    # identite SEL + titre de document
    logo=Path(getattr(sys,'_MEIPASS',Path(__file__).resolve().parent))/'static'/'sel-icon.png'
    # L’icône fournie par SEL est embarquée telle quelle : aucune retouche ni rognage.
    mark=Image(str(logo),width=20*mm,height=20*mm,hAlign='LEFT') if logo.exists() else Paragraph('<b>SEL</b>',title)
    company_cells=[mark,
        [Paragraph('<b>'+p(co.get('company_name','SOCIÉTÉ D’ÉNERGIE DU LUALABA'))+'</b>',ParagraphStyle('co',parent=normal,fontName='Helvetica-Bold',fontSize=10.8,leading=13,textColor=BLUE)),
         Paragraph(p(co.get('tagline','')),muted),
         Paragraph(p(co.get('address','')),small),
         Paragraph(' · '.join([p(v) for v in (co.get('phone'),co.get('email'),co.get('website')) if v]),small)],
        [Paragraph('DÉCOMPTE D’ÉLECTRICITÉ',document_title),
         Paragraph('N° '+p(inv['number']),ParagraphStyle('ref',parent=normal,fontName='Helvetica-Bold',fontSize=9,alignment=TA_RIGHT,textColor=BLUE)),
         Paragraph('Émis le '+p(inv['issue_date']),right)]]
    table=Table([company_cells],colWidths=[26*mm,94*mm,58*mm])
    table.setStyle(TableStyle([('VALIGN',(0,0),(-1,-1),'MIDDLE'),('LEFTPADDING',(0,0),(-1,-1),0),('RIGHTPADDING',(0,0),(-1,-1),4),('TOPPADDING',(0,0),(-1,-1),1),('BOTTOMPADDING',(0,0),(-1,-1),8),('LINEBELOW',(0,0),(-1,-1),2,BLUE)]))
    story.extend([table,Spacer(1,7)])
    # Les mentions de conformité internes n'appartiennent pas au document client.
    # Aucun logo, QR code, numéro ou certification DGI fictif n'est généré.

    info_left='<b>INFORMATIONS CLIENT</b><br/>' + '<br/>'.join([
         f"<b>Nom :</b> {p(client.get('name'))}",f"<b>Code :</b> {p(client.get('code'))}",
         f"<b>Contrat :</b> {p(client.get('contract_number'))}",f"<b>Adresse :</b> {p(client.get('address'))}",
             f"<b>Type d’abonné :</b> {p(d.get('customer_type_label') or {'RESIDENTIAL':'Résidentiel','COMMERCIAL':'Commercial'}.get(d.get('customer_type'),client.get('category')))}",f"<b>NIF client :</b> {p(client.get('nif'))}"])
    direct_billing = d.get('pricing_model', 'METER') == 'FLAT_KWH'
    if direct_billing:
        info_right='<b>PÉRIODE ET FACTURATION</b><br/>'+'<br/>'.join([
             f"<b>Centre :</b> {p(client.get('center'))}",
             f"<b>Point de service :</b> {p(client.get('service_point'))}",
             f"<b>Type d’abonné :</b> {p({'RESIDENTIAL':'Résidentiel','COMMERCIAL':'Commercial'}.get(d.get('customer_type'),client.get('category')))}",
             f"<b>Période :</b> {p(period_fr(inv['billing_period']))}",
             f"<b>Échéance :</b> {p(inv['due_date'])}",
             '<b>Mode :</b> Consommation directe'])
    else:
        info_right='<b>INFORMATIONS COMPTEUR</b><br/>'+'<br/>'.join([
             f"<b>Centre :</b> {p(client.get('center'))}",f"<b>Point de service :</b> {p(client.get('service_point'))}",
             f"<b>N° compteur :</b> {p(client.get('meter_number'))}",f"<b>Coefficient :</b> {p(d.get('coefficient'))}",
             f"<b>Période :</b> {p(period_fr(inv['billing_period']))}",f"<b>Échéance :</b> {p(inv['due_date'])}"])
    details=Table([[Paragraph(info_left,normal),Paragraph(info_right,normal)]],colWidths=[89*mm,89*mm])
    details.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,-1),colors.HexColor('#F5F9FB')),('BOX',(0,0),(-1,-1),.6,BORDER),('INNERGRID',(0,0),(-1,-1),.4,BORDER),('VALIGN',(0,0),(-1,-1),'TOP'),('TOPPADDING',(0,0),(-1,-1),11),('BOTTOMPADDING',(0,0),(-1,-1),11),('LEFTPADDING',(0,0),(-1,-1),11)]))
    story.extend([details,Paragraph('RELEVÉ ET CONSOMMATION',section)])
    if direct_billing:
        usage=Table([[Paragraph('Type d’abonné',twhite),Paragraph('Consommation',twhite),Paragraph('Période facturée',twhite),Paragraph('Échéance',twhite)],
                     [p({'RESIDENTIAL':'Résidentiel','COMMERCIAL':'Commercial'}.get(d.get('customer_type'),client.get('category'))),p(d['kwh'])+' kWh',p(period_fr(inv['billing_period'])),p(inv['due_date'])]],
                    colWidths=[43*mm,43*mm,43*mm,49*mm])
    else:
        usage=Table([[Paragraph('Index précédent',twhite),Paragraph('Index actuel',twhite),Paragraph('Consommation',twhite),Paragraph('Période facturée',twhite)],
                     [p(d.get('previous') or '—'),p(d.get('current') or '—'),p(d['kwh'])+' kWh',p(period_fr(inv['billing_period']))]],colWidths=[43*mm,43*mm,43*mm,49*mm])
    usage.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),BLUE),('LINEBELOW',(0,0),(-1,0),.8,BLUE),('INNERGRID',(0,0),(-1,-1),.45,BORDER),('BOX',(0,0),(-1,-1),.65,BORDER),('ALIGN',(0,0),(-1,-1),'CENTER'),('VALIGN',(0,0),(-1,-1),'MIDDLE'),('TOPPADDING',(0,0),(-1,-1),8),('BOTTOMPADDING',(0,0),(-1,-1),8)]))
    story.extend([usage,Paragraph('DÉTAIL DE LA CONSOMMATION FACTURÉE',section)])
    lines=[[Paragraph('Tranche tarifaire',twhite),Paragraph('Quantité (kWh)',twhite),Paragraph('Prix unitaire',twhite),Paragraph('Montant',twhite)]]
    for row in d['lines']:
        lines.append([p(row['range']),p(row['quantity']),rate_fmt(row['unit_price']),fmt(row['amount'])])
    if not d['lines']:lines.append(['Aucune consommation','0','—','0,00'])
    table=Table(lines,colWidths=[49*mm,44*mm,43*mm,42*mm],repeatRows=1)
    table.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),BLUE),('ROWBACKGROUNDS',(0,1),(-1,-1),[colors.white,colors.HexColor('#F7FAFC')]),('INNERGRID',(0,0),(-1,-1),.4,BORDER),('BOX',(0,0),(-1,-1),.6,BORDER),('VALIGN',(0,0),(-1,-1),'MIDDLE'),('ALIGN',(1,1),(-1,-1),'RIGHT'),('LEFTPADDING',(0,0),(-1,-1),8),('RIGHTPADDING',(0,0),(-1,-1),8),('TOPPADDING',(0,0),(-1,-1),7),('BOTTOMPADDING',(0,0),(-1,-1),7)]))
    story.append(table)
    recap=[('Sous-total énergie',fmt(d['energy']))]
    if Decimal(str(d.get('local_tax','0'))) != 0:
        recap.append((f"Taxe locale ({p(d['local_rate'])} %)",fmt(d['local_tax'])))
    if Decimal(str(d.get('fee','0'))) != 0:
        recap.append(('Frais supplémentaires',fmt(d['fee'])))
    if d.get('unit_price'):
        recap.append(('Tarif unitaire HT',f"{rate_fmt(d['unit_price'])} {p(inv['currency'])}/kWh"))
    if d.get('conversion_rate'):
        recap.append(('Conversion indicative',f"1 {p(d.get('conversion_source_currency','USD'))} = {rate_fmt(d['conversion_rate'])} {p(inv['currency'])}"))
    recap.extend([(f"TVA ({p(d['vat_rate'])} %) — base {fmt(d['vat_basis'])}",fmt(d['vat'])),
           ('MONTANT TOTAL ('+p(inv['currency'])+')',fmt(d['total']))])
    rec=Table([[Paragraph(a,total_label if a.startswith('MONTANT') else normal),Paragraph(b,total_value if a.startswith('MONTANT') else right)] for a,b in recap],colWidths=[130*mm,48*mm])
    rec.setStyle(TableStyle([('LINEABOVE',(0,-1),(-1,-1),1.2,BLUE),('BACKGROUND',(0,-1),(-1,-1),PALE),('TOPPADDING',(0,0),(-1,-1),6),('BOTTOMPADDING',(0,0),(-1,-1),6),('RIGHTPADDING',(1,0),(1,-1),8),('LEFTPADDING',(0,0),(0,-1),8)]))
    story.extend([Spacer(1,7),rec,Spacer(1,10)])
    payment_box=Table([[Paragraph('CONDITIONS DE RÈGLEMENT',twhite),Paragraph('SITUATION DU COMPTE',twhite)],
                       [Paragraph(f"Échéance : <b>{p(inv['due_date'])}</b><br/>Paiement accepté selon les modalités de SEL.",normal),
                        Paragraph(f"Déjà réglé : <b>{fmt(paid)} {p(inv['currency'])}</b><br/>MONTANT À RÉGLER : <b>{fmt(balance)} {p(inv['currency'])}</b>",normal)]],
                      colWidths=[89*mm,89*mm])
    payment_box.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),BLUE),('BACKGROUND',(0,1),(-1,1),colors.HexColor('#F5F9FB')),('BOX',(0,0),(-1,-1),.65,BORDER),('INNERGRID',(0,0),(-1,-1),.45,BORDER),('VALIGN',(0,0),(-1,-1),'TOP'),('TOPPADDING',(0,0),(-1,-1),8),('BOTTOMPADDING',(0,0),(-1,-1),8),('LEFTPADDING',(0,0),(-1,-1),9),('RIGHTPADDING',(0,0),(-1,-1),9)]))
    story.append(payment_box)
    if inv['status']=='VOID':story.append(Paragraph('<font color="red"><b>DOCUMENT ANNULÉ - NE PAS UTILISER</b></font>',section))
    if inv['note']:story.extend([Spacer(1,6),Paragraph('<b>Note :</b> '+p(inv['note']),small)])
    story.extend([Spacer(1,10),Paragraph('<b>MENTIONS ENTREPRISE :</b> RCCM : '+p(co.get('rccm'))+' &nbsp;&nbsp; NIF : '+p(co.get('nif'))+' &nbsp;&nbsp; ID NAT : '+p(co.get('id_nat')),small),
                  ])
    def footer(canvas, document):
        canvas.saveState()
        w,h=A4
        canvas.setStrokeColor(BORDER);canvas.line(15*mm,14*mm,w-15*mm,14*mm)
        canvas.setFont('Helvetica',7);canvas.setFillColor(GREY)
        canvas.drawString(15*mm,10*mm,'SEL - Service clientèle  |  '+str(inv['number']))
        canvas.drawRightString(w-15*mm,10*mm,f'Page {document.page}')
        canvas.restoreState()
    doc.build(story,onFirstPage=footer,onLaterPages=footer)
    return out.getvalue()
